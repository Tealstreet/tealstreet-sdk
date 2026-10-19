//! Borrowed Bitunix wire records. No intermediate JSON object graph.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::{borrow::Cow, fmt};
pub type Str<'a> = Cow<'a, str>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Num(pub f64);
impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Number;
        impl Visitor<'_> for Number {
            type Value = Num;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number or decimal string")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
                Ok(Num(if v.is_finite() { v } else { 0.0 }))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Num, E> {
                Ok(Num(safe_cex_engine::num::parse(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Num, E> {
                Ok(Num(0.0))
            }
        }
        d.deserialize_any(Number)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Supported<'a> {
    Bool(bool),
    Number(f64),
    Text(#[serde(borrow)] Str<'a>),
}
impl Supported<'_> {
    pub fn boolean(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            Self::Text(v) if v.eq_ignore_ascii_case("true") => Some(true),
            Self::Text(v) if v.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }

    pub fn allowed(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Number(v) => *v != 0.0,
            Self::Text(v) => !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "n" | "disabled" | "unsupported"
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base: Str<'a>,
    #[serde(borrow)]
    pub quote: Str<'a>,
    #[serde(borrow)]
    pub symbol_status: Str<'a>,
    pub base_precision: Num,
    pub quote_precision: Num,
    pub max_limit_order_volume: Num,
    #[serde(borrow, default)]
    pub is_api_supported: Option<Supported<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub last: Num,
    pub open: Num,
    pub mark_price: Num,
    pub base_vol: Num,
    pub quote_vol: Num,
}
#[derive(Deserialize, Default)]
pub struct Message<'a> {
    #[serde(borrow, default, deserialize_with = "text")]
    pub ch: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub channel: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub op: Str<'a>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
pub struct Tick {
    pub la: Num,
    pub o: Num,
    pub b: Num,
    pub q: Num,
}
#[derive(Deserialize)]
pub struct Price {
    pub mp: Num,
    pub ip: Num,
    pub fr: Num,
    pub ft: Num,
}
#[derive(Deserialize)]
pub struct Depth {
    pub bids: Vec<[Num; 2]>,
    pub asks: Vec<[Num; 2]>,
}
#[derive(Deserialize)]
pub struct Top {
    pub b: Vec<[Num; 2]>,
    pub a: Vec<[Num; 2]>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    pub time: Num,
    pub open: Num,
    pub high: Num,
    pub low: Num,
    pub close: Num,
    #[serde(default)]
    pub volume: Num,
    #[serde(default)]
    pub quote_vol: Num,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: Num,
    #[serde(borrow, default, deserialize_with = "text")]
    pub msg: Str<'a>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
pub fn data(text: &str) -> Result<&str, String> {
    let e: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if e.code.0 != 0.0 {
        return Err(e.msg.into_owned());
    }
    e.data
        .map(|r| r.get())
        .ok_or_else(|| "Bitunix response has no data".into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Account<'a> {
    #[serde(borrow, default, deserialize_with = "text")]
    pub margin_coin: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "posMode")]
    pub position_mode: Str<'a>,
    #[serde(default, alias = "availableBalance", alias = "availableMargin")]
    pub available: Option<Num>,
    #[serde(default, alias = "used", alias = "usedMargin")]
    pub margin: Option<Num>,
    #[serde(default, alias = "locked")]
    pub frozen: Option<Num>,
    #[serde(default, alias = "crossUnrealizedPNL")]
    pub cross_unrealized_pnl: Option<Num>,
    #[serde(default, alias = "isolationUnrealizedPNL")]
    pub isolation_unrealized_pnl: Option<Num>,
    #[serde(default, alias = "unrealizedPNL", alias = "upnl")]
    pub unrealized_pnl: Option<Num>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(
        borrow,
        default,
        alias = "instId",
        alias = "contract",
        alias = "instrument"
    )]
    pub symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub position_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "positionSide")]
    pub side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "marginType")]
    pub margin_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "posMode")]
    pub position_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub event: Str<'a>,
    #[serde(default)]
    pub qty: Option<Num>,
    #[serde(default)]
    pub max_qty: Option<Num>,
    #[serde(default)]
    pub size: Option<Num>,
    #[serde(default)]
    pub position_amt: Option<Num>,
    #[serde(default)]
    pub amount: Option<Num>,
    #[serde(default)]
    pub contracts: Option<Num>,
    #[serde(default)]
    pub avg_open_price: Option<Num>,
    #[serde(default)]
    pub entry_price: Option<Num>,
    #[serde(default)]
    pub open_price: Option<Num>,
    #[serde(default)]
    pub avg_price: Option<Num>,
    #[serde(default)]
    pub entry_value: Option<Num>,
    #[serde(default)]
    pub notional: Option<Num>,
    #[serde(default)]
    pub position_value: Option<Num>,
    #[serde(default, alias = "lever")]
    pub leverage: Num,
    #[serde(
        default,
        alias = "unrealizedPNL",
        alias = "unRealizedProfit",
        alias = "upl"
    )]
    pub unrealized_pnl: Num,
    #[serde(default, alias = "realizedPNL")]
    pub realized_pnl: Num,
    #[serde(default, alias = "liquidationPrice", alias = "liqPx")]
    pub liq_price: Num,
    #[serde(default)]
    pub fee: Num,
    #[serde(default)]
    pub funding: Num,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow, default, deserialize_with = "text")]
    pub symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub position_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "type")]
    pub kind: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub client_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub effect: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub event: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "status")]
    pub order_status: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub tp_stop_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub sl_stop_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub tp_order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub sl_order_type: Str<'a>,
    #[serde(default)]
    pub price: Num,
    #[serde(default)]
    pub qty: Num,
    #[serde(default)]
    pub trade_qty: Num,
    #[serde(default)]
    pub deal_amount: Num,
    #[serde(default, alias = "avgPrice")]
    pub average_price: Num,
    #[serde(borrow, default, deserialize_with = "text")]
    pub mtime: Str<'a>,
    #[serde(default)]
    pub tp_price: Option<Num>,
    #[serde(default)]
    pub sl_price: Option<Num>,
    #[serde(default)]
    pub tp_qty: Num,
    #[serde(default)]
    pub sl_qty: Num,
    #[serde(default)]
    pub tp_order_price: Num,
    #[serde(default)]
    pub sl_order_price: Num,
    #[serde(borrow, default, alias = "reductionOnly")]
    pub reduce_only: Option<Supported<'a>>,
}

impl PositionRow<'_> {
    pub fn quantity(&self) -> f64 {
        self.qty
            .or(self.max_qty)
            .or(self.size)
            .or(self.position_amt)
            .or(self.amount)
            .or(self.contracts)
            .unwrap_or_default()
            .0
    }
    pub fn entry(&self) -> f64 {
        self.avg_open_price
            .or(self.entry_price)
            .or(self.open_price)
            .or(self.avg_price)
            .unwrap_or_default()
            .0
    }
    pub fn notional(&self) -> f64 {
        self.entry_value
            .or(self.notional)
            .or(self.position_value)
            .unwrap_or_default()
            .0
    }
}

pub(crate) fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    struct Text;
    impl<'de> Visitor<'de> for Text {
        type Value = Str<'de>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("text or null")
        }
        fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(v))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.into()))
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(""))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.to_string()))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.to_string()))
        }
    }
    d.deserialize_any(Text)
}

pub fn time_ms(s: &str) -> f64 {
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis() as f64)
        })
        .unwrap_or(0.0)
}

/// Required quantities on mutation recovery must never default malformed input to zero.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct StrictNum(#[serde(deserialize_with = "strict_number")] pub f64);
fn strict_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Numeric<'a> {
        Number(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    let value = match Numeric::deserialize(d)? {
        Numeric::Number(v) => v,
        Numeric::Text(s) => s.parse::<f64>().map_err(de::Error::custom)?,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(de::Error::custom("nonfinite quantity"))
    }
}
