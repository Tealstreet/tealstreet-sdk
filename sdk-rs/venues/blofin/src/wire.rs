//! Borrowed BloFin payloads; unknown fields never enter the store.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

// BloFin sends "" for values that do not apply; malformed text reads as 0.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Number(#[serde(deserialize_with = "safe_cex_engine::de::lenient_number")] pub f64);

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow)]
    pub inst_type: Str<'a>,
    #[serde(borrow)]
    pub contract_type: Str<'a>,
    #[serde(borrow)]
    pub base_currency: Str<'a>,
    #[serde(borrow)]
    pub quote_currency: Str<'a>,
    #[serde(borrow)]
    pub state: Str<'a>,
    pub contract_value: Number,
    pub min_size: Number,
    #[serde(default)]
    pub lot_size: Number,
    pub tick_size: Number,
    pub max_limit_size: Number,
    pub max_market_size: Number,
    pub max_leverage: Number,
}
/// The identity of an instrument row that failed to decode.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentKey<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow, default)]
    pub inst_type: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub contract_type: Option<Str<'a>>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tick<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(default)]
    pub last: Number,
    #[serde(default)]
    pub open24h: Number,
    #[serde(default)]
    pub bid_price: Number,
    #[serde(default)]
    pub ask_price: Number,
    #[serde(default)]
    pub vol_currency24h: Number,
    #[serde(default)]
    pub vol24h: Number,
    #[serde(default)]
    pub funding_rate: Number,
    #[serde(default)]
    pub funding_time: Number,
}
#[derive(Deserialize, Default)]
pub struct Arg<'a> {
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default, rename = "instId")]
    pub inst_id: Str<'a>,
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub arg: Arg<'a>,
    #[serde(borrow, default)]
    pub action: Str<'a>,
    #[serde(borrow, default)]
    pub event: Str<'a>,
    #[serde(borrow, default)]
    pub code: Str<'a>,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}
#[derive(Deserialize)]
pub struct Data<T> {
    pub data: T,
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
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub bids: Vec<Vec<Number>>,
    pub asks: Vec<Vec<Number>>,
    #[serde(default)]
    pub seq_id: Option<Sequence>,
    #[serde(default)]
    pub prev_seq_id: Option<Sequence>,
    #[serde(default)]
    pub ts: Number,
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub code: Str<'a>,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}
pub fn check(response: &safe_cex_engine::Response<'_>) -> Result<(), String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "BloFin HTTP {}: {}",
            response.status, response.body
        ));
    }
    let envelope: Envelope<'_> = serde_json::from_str(response.body).map_err(|e| e.to_string())?;
    if envelope.code != "0" {
        return Err(format!("BloFin {}: {}", envelope.code, envelope.msg));
    }
    Ok(())
}

#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "SequenceWire<'_>")]
pub struct Sequence(pub u64);
#[derive(Deserialize)]
#[serde(untagged)]
enum SequenceWire<'a> {
    Bare(u64),
    Text(#[serde(borrow)] Str<'a>),
}
impl TryFrom<SequenceWire<'_>> for Sequence {
    type Error = String;
    fn try_from(value: SequenceWire<'_>) -> Result<Self, Self::Error> {
        match value {
            SequenceWire::Bare(v) => Ok(Self(v)),
            SequenceWire::Text(v) => v
                .parse()
                .map(Self)
                .map_err(|_| "Invalid book sequence".into()),
        }
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum CandleWire {
    Row(Vec<Number>),
    Object(CandleObject),
}
#[derive(Deserialize)]
pub struct CandleObject {
    #[serde(alias = "timestamp", alias = "t", alias = "time")]
    pub ts: Number,
    #[serde(alias = "o")]
    pub open: Number,
    #[serde(alias = "h")]
    pub high: Number,
    #[serde(alias = "l")]
    pub low: Number,
    #[serde(alias = "c")]
    pub close: Number,
    #[serde(default, alias = "vol", alias = "v")]
    pub volume: Number,
}
impl CandleWire {
    pub fn candle(&self) -> Option<safe_cex_engine::model::Candle> {
        match self {
            Self::Row(row) => super::candle(row),
            Self::Object(c) if c.ts.0 > 0.0 => Some(safe_cex_engine::model::Candle {
                timestamp: if c.ts.0 > 1e12 {
                    c.ts.0 / 1000.0
                } else {
                    c.ts.0
                },
                open: c.open.0,
                high: c.high.0,
                low: c.low.0,
                close: c.close.0,
                volume: c.volume.0,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(from = "BooleanWire<'_>")]
pub struct Boolean(pub bool);
#[derive(Deserialize)]
#[serde(untagged)]
enum BooleanWire<'a> {
    Bare(bool),
    Text(#[serde(borrow)] Str<'a>),
}
impl From<BooleanWire<'_>> for Boolean {
    fn from(v: BooleanWire<'_>) -> Self {
        Self(match v {
            BooleanWire::Bare(v) => v,
            BooleanWire::Text(v) => v == "true",
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow, default, deserialize_with = "nullable_attachments")]
    pub attach_algo_orders: Vec<Attachment<'a>>,
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow, default)]
    pub inst_type: Str<'a>,
    #[serde(borrow, default)]
    pub order_id: Str<'a>,
    #[serde(borrow, default)]
    pub ord_id: Str<'a>,
    #[serde(borrow, default)]
    pub tpsl_id: Str<'a>,
    #[serde(borrow, default)]
    pub algo_id: Str<'a>,
    #[serde(borrow, default)]
    pub order_category: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub order_type: Str<'a>,
    #[serde(borrow, default)]
    pub state: Str<'a>,
    #[serde(borrow, default)]
    pub side: Str<'a>,
    #[serde(borrow, default)]
    pub position_side: Str<'a>,
    #[serde(borrow, default)]
    pub margin_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub client_order_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub trigger_price_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub tp_trigger_price_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub sl_trigger_price_type: Str<'a>,
    #[serde(default)]
    pub price: Number,
    #[serde(default)]
    pub size: Number,
    #[serde(default)]
    pub filled_size: Number,
    #[serde(default)]
    pub average_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub trigger_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub order_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub tp_trigger_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub sl_trigger_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub tp_order_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub sl_order_price: Number,
    #[serde(default)]
    pub reduce_only: Boolean,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow, default)]
    pub inst_type: Str<'a>,
    #[serde(borrow, default)]
    pub position_side: Str<'a>,
    #[serde(borrow, default)]
    pub margin_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub liquidation_price: Str<'a>,
    #[serde(default)]
    pub positions: Number,
    #[serde(default)]
    pub average_price: Number,
    #[serde(default)]
    pub mark_price: Number,
    #[serde(default)]
    pub unrealized_pnl: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub leverage: Number,
    #[serde(default)]
    pub maintenance_margin: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetRow {
    pub available: Number,
    pub frozen: Number,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageRow<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow)]
    pub margin_mode: Str<'a>,
    #[serde(borrow)]
    pub position_side: Str<'a>,
    pub leverage: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModeRow<'a> {
    #[serde(borrow, default)]
    pub position_mode: Str<'a>,
    #[serde(borrow, default)]
    pub margin_mode: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    pub total_equity: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Attachment<'a> {
    #[serde(default, deserialize_with = "nullable_number")]
    pub tp_trigger_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub sl_trigger_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub tp_order_price: Number,
    #[serde(default, deserialize_with = "nullable_number")]
    pub sl_order_price: Number,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub tp_trigger_price_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    pub sl_trigger_price_type: Str<'a>,
}

pub fn nullable_text<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    Ok(Option::<Str<'de>>::deserialize(d)?.unwrap_or_default())
}

pub fn optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Number>, D::Error> {
    Ok(safe_cex_engine::de::lenient_optional_number(d)?.map(Number))
}

// Optional prices use zero as the existing "no attached protection" sentinel.
// Required financial fields deliberately continue to reject JSON null.
fn nullable_number<'de, D: Deserializer<'de>>(d: D) -> Result<Number, D::Error> {
    Ok(Option::<Number>::deserialize(d)?.unwrap_or_default())
}

fn nullable_attachments<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Attachment<'de>>, D::Error> {
    Ok(Option::<Vec<Attachment<'de>>>::deserialize(d)?.unwrap_or_default())
}
