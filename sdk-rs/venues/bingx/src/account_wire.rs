//! Private REST and stream records. IDs use integer visitors to preserve all bits.
use crate::wire::{Str, number};
pub use safe_cex_engine::de::optional_number;
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt;

pub fn identifier<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    struct Id;
    impl<'de> serde::de::Visitor<'de> for Id {
        type Value = Str<'de>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or integer identifier")
        }
        fn visit_borrowed_str<E: serde::de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(s))
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(s.into()))
        }
        fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
        fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
    }
    d.deserialize_any(Id)
}

pub fn boolean<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Bool<'a> {
        Bare(bool),
        Text(#[serde(borrow)] Str<'a>),
    }
    match Bool::deserialize(d)? {
        Bool::Bare(b) => Ok(b),
        Bool::Text(s) => match s.as_ref() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(serde::de::Error::custom("invalid boolean")),
        },
    }
}

#[derive(Deserialize)]
pub struct Status<'a> {
    pub code: i64,
    #[serde(default, borrow)]
    pub msg: Str<'a>,
}

#[derive(Deserialize)]
pub struct KeyStatus {
    #[serde(default, rename = "code")]
    _code: Option<i64>,
}

#[derive(Deserialize)]
pub struct ListenKey<'a> {
    #[serde(default, borrow, rename = "listenKey")]
    pub key: Option<Str<'a>>,
    #[serde(default, borrow)]
    pub data: Option<Box<ListenKey<'a>>>,
}
impl ListenKey<'_> {
    pub fn key(&self) -> Option<&str> {
        self.key
            .as_deref()
            .or_else(|| self.data.as_ref().and_then(|d| d.key()))
    }
}

#[derive(Deserialize)]
pub struct ModeEnvelope {
    pub data: Mode,
}
#[derive(Deserialize)]
pub struct Mode {
    #[serde(rename = "dualSidePosition", deserialize_with = "boolean")]
    pub dual: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow<'a> {
    #[serde(borrow)]
    pub asset: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub balance: f64,
    #[serde(deserialize_with = "number")]
    pub available_margin: f64,
    #[serde(deserialize_with = "number")]
    pub used_margin: f64,
    #[serde(deserialize_with = "number")]
    pub unrealized_profit: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub position_side: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub position_amt: f64,
    #[serde(deserialize_with = "number")]
    pub avg_price: f64,
    #[serde(deserialize_with = "number")]
    pub unrealized_profit: f64,
    #[serde(deserialize_with = "number")]
    pub leverage: f64,
    #[serde(deserialize_with = "boolean")]
    pub isolated: bool,
    #[serde(deserialize_with = "number")]
    pub liquidation_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub isolated_margin: f64,
    #[serde(default, deserialize_with = "number")]
    pub initial_margin: f64,
}

#[derive(Deserialize)]
pub struct PositionIdentity<'a> {
    #[serde(default, borrow)]
    pub symbol: Str<'a>,
    #[serde(default, borrow, rename = "positionSide")]
    pub position_side: Str<'a>,
}

#[derive(Deserialize)]
pub struct OrderIdentity<'a> {
    #[serde(default, borrow, rename = "orderId", deserialize_with = "identifier")]
    pub order_id: Str<'a>,
}

#[derive(Deserialize)]
pub struct Protection {
    #[serde(default, rename = "stopPrice", deserialize_with = "number")]
    stop_price: f64,
    #[serde(default, deserialize_with = "number")]
    price: f64,
    #[serde(default, deserialize_with = "number")]
    quantity: f64,
}
impl Protection {
    pub fn meaningful(&self) -> bool {
        self.stop_price > 0.0 || self.price > 0.0 || self.quantity > 0.0
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOrder<'a> {
    #[serde(default, borrow, deserialize_with = "identifier")]
    pub order_id: Str<'a>,
    #[serde(default, borrow, rename = "orderID", deserialize_with = "identifier")]
    pub legacy_order_id: Str<'a>,
    #[serde(default, borrow, rename = "i", deserialize_with = "identifier")]
    pub short_id: Str<'a>,
    #[serde(borrow, alias = "s")]
    pub symbol: Str<'a>,
    #[serde(borrow, rename = "type", alias = "o")]
    pub kind: Str<'a>,
    #[serde(borrow, alias = "S")]
    pub side: Str<'a>,
    #[serde(default, borrow, alias = "X")]
    pub status: Option<Str<'a>>,
    #[serde(
        default,
        alias = "origQty",
        alias = "q",
        deserialize_with = "optional_number"
    )]
    pub quantity: Option<f64>,
    #[serde(default, alias = "p", deserialize_with = "number")]
    pub price: f64,
    #[serde(default, alias = "sp", deserialize_with = "number")]
    pub stop_price: f64,
    #[serde(
        default,
        alias = "executedQty",
        alias = "z",
        deserialize_with = "optional_number"
    )]
    pub filled: Option<f64>,
    #[serde(default, alias = "l", deserialize_with = "number")]
    pub last_quantity: f64,
    #[serde(default, alias = "L", deserialize_with = "number")]
    pub last_price: f64,
    #[serde(default, alias = "ap", deserialize_with = "number")]
    pub avg_price: f64,
    #[serde(default, alias = "R", alias = "ro", deserialize_with = "boolean")]
    pub reduce_only: bool,
    #[serde(default, alias = "cp", deserialize_with = "boolean")]
    pub close_position: bool,
    #[serde(default, deserialize_with = "boolean")]
    pub post_only: bool,
    #[serde(default, borrow, alias = "c")]
    pub client_order_id: Option<Str<'a>>,
    #[serde(default, borrow, alias = "f")]
    pub time_in_force: Option<Str<'a>>,
    #[serde(default, borrow, alias = "wt")]
    pub working_type: Option<Str<'a>>,
    #[serde(default, borrow, alias = "ps")]
    pub position_side: Str<'a>,
    #[serde(default, borrow, alias = "t", deserialize_with = "identifier")]
    pub trade_id: Str<'a>,
    #[serde(default, alias = "T", deserialize_with = "number")]
    pub time: f64,
    #[serde(default)]
    pub take_profit: Option<Protection>,
    #[serde(default)]
    pub stop_loss: Option<Protection>,
}

impl NativeOrder<'_> {
    pub fn id(&self) -> Option<&str> {
        let mut ids = [
            self.order_id.as_ref(),
            self.legacy_order_id.as_ref(),
            self.short_id.as_ref(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty());
        let first = ids.next()?;
        ids.all(|id| id == first).then_some(first)
    }
    pub fn complete_metadata(&self) -> bool {
        self.quantity.is_some()
            && self.filled.is_some()
            && self.time_in_force.is_some()
            && !self.position_side.is_empty()
    }
}

#[derive(Default, Deserialize)]
pub struct AccountUpdate<'a> {
    #[serde(default, borrow, rename = "P")]
    pub positions: Vec<&'a serde_json::value::RawValue>,
    #[serde(default, borrow, rename = "p")]
    pub legacy_positions: Vec<&'a serde_json::value::RawValue>,
    #[serde(default, borrow, rename = "B")]
    pub balances: Vec<&'a serde_json::value::RawValue>,
}

// The fallback B array can contain balance records; only symbol-bearing rows
// with a position quantity qualify as a position.
#[derive(Deserialize)]
pub struct PositionUpdate<'a> {
    #[serde(default, borrow)]
    pub s: Str<'a>,
    #[serde(default, alias = "positionAmt", deserialize_with = "optional_number")]
    pub pa: Option<f64>,
    #[serde(default, alias = "avgPrice", deserialize_with = "optional_number")]
    pub ep: Option<f64>,
    #[serde(
        default,
        alias = "unrealizedProfit",
        deserialize_with = "optional_number"
    )]
    pub up: Option<f64>,
    #[serde(
        default,
        alias = "liquidationPrice",
        deserialize_with = "optional_number"
    )]
    pub liqp: Option<f64>,
    #[serde(default, borrow)]
    pub ps: Str<'a>,
    #[serde(default, borrow)]
    pub mt: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct Config<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(default, rename = "S", deserialize_with = "number")]
    pub short: f64,
    #[serde(default, borrow)]
    pub mt: Option<Str<'a>>,
}
#[derive(Deserialize)]
pub struct UserEvent<'a> {
    #[serde(default, borrow)]
    pub e: Str<'a>,
    #[serde(default, rename = "E", deserialize_with = "number")]
    pub time: f64,
    #[serde(default, borrow)]
    pub a: AccountUpdate<'a>,
    #[serde(default, borrow)]
    pub o: Option<NativeOrder<'a>>,
    #[serde(default, borrow)]
    pub ac: Option<Config<'a>>,
    #[serde(default)]
    pub ping: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    pub pong: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Placed<'a> {
    Nested {
        #[serde(borrow)]
        order: NativeOrder<'a>,
    },
    Direct(#[serde(borrow)] NativeOrder<'a>),
}
#[derive(Deserialize)]
pub struct PlacedEnvelope<'a> {
    #[serde(borrow)]
    pub data: Placed<'a>,
}

#[derive(Deserialize)]
pub struct Data<T> {
    pub data: T,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leverage {
    #[serde(deserialize_with = "number")]
    pub long_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub short_leverage: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginMode<'a> {
    #[serde(borrow)]
    pub margin_type: Str<'a>,
}

#[derive(Deserialize)]
pub struct PositionIntent<'a> {
    #[serde(default, borrow)]
    pub s: Str<'a>,
}
