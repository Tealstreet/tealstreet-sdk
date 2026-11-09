//! Canonical Balance contracts, ordered dictionaries and retained-info codecs.
use crate::retained_json::RetainedJsonInput as Input;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct CoinBalance {
    /// @ts-mutable
    pub base: String,
    /// @ts-mutable
    pub amount: f64,
    /// @ts-mutable
    pub notional: f64,
}

/// A currency row preserves the shape returned by its source venue. Crypto.com
/// REST returns available/locked rather than the statically declared Balance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CurrencyBalance {
    Declared(Balance),
    AvailableLocked { available: f64, locked: f64 },
}

impl CurrencyBalance {
    /// Read a declared Balance row without normalizing other source shapes.
    pub fn as_balance(&self) -> Option<&Balance> {
        match self {
            Self::Declared(balance) => Some(balance),
            Self::AvailableLocked { .. } => None,
        }
    }
}
impl From<Balance> for CurrencyBalance {
    fn from(balance: Balance) -> Self {
        Self::Declared(balance)
    }
}
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Balance {
    pub used: f64,
    pub free: f64,
    pub total: f64,
    pub upnl: f64,

    #[ts(type = "any", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,

    #[ts(type = "{ [currency: string]: Balance }", optional)]
    pub currencies: Option<crate::object_map::JsObjectMap<CurrencyBalance>>,

    #[ts(type = "{ [coin: string]: CoinBalance }", optional)]
    pub coins: Option<crate::object_map::JsObjectMap<CoinBalance>>,
}

#[derive(serde::Deserialize)]
#[serde(remote = "CoinBalance")]
#[serde(rename_all = "camelCase")]
struct WireCoinBalance {
    base: String,
    amount: f64,
    notional: f64,
}

impl<'de> serde::Deserialize<'de> for CoinBalance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireCoinBalance::deserialize(crate::strict_decode::object(deserializer))
    }
}

#[derive(serde::Deserialize)]
struct AvailableLockedBalance {
    available: f64,
    locked: f64,
}

impl<'de> serde::Deserialize<'de> for CurrencyBalance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = crate::retained_json::RetainedJsonInput::deserialize(deserializer)?;
        if !input.is_object() {
            return Err(serde::de::Error::custom(
                "CurrencyBalance must be an object",
            ));
        }
        if let Ok(balance) = input.clone().decode_native::<Balance>(false) {
            return Ok(Self::Declared(balance));
        }
        let value = input
            .decode_native::<AvailableLockedBalance>(false)
            .map_err(serde::de::Error::custom)?;
        Ok(Self::AvailableLocked {
            available: value.available,
            locked: value.locked,
        })
    }
}

#[derive(serde::Deserialize)]
#[serde(remote = "Balance")]
#[serde(rename_all = "camelCase")]
struct WireBalance {
    used: f64,
    free: f64,
    total: f64,
    upnl: f64,
    #[serde(
        default,
        deserialize_with = "crate::retained_json::deserialize_present_retained_info",
        skip_serializing_if = "Option::is_none"
    )]
    info: Option<crate::retained_json::RetainedJsonInput>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    currencies: Option<crate::object_map::JsObjectMap<crate::CurrencyBalance>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    coins: Option<crate::object_map::JsObjectMap<CoinBalance>>,
}

impl<'de> serde::Deserialize<'de> for Balance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InfoVisitor;
        impl<'de> serde::de::Visitor<'de> for InfoVisitor {
            type Value = Balance;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a Balance record")
            }
            fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Self::Value, D::Error> {
                WireBalance::deserialize(crate::strict_decode::object(d))
            }
        }
        deserializer.deserialize_newtype_struct(crate::model_field::INFO_RECORD, InfoVisitor)
    }
}
impl serde::Serialize for Balance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::INFO_RECORD, 7)?;
        state.serialize_field("used", &self.used)?;
        state.serialize_field("free", &self.free)?;
        state.serialize_field("total", &self.total)?;
        state.serialize_field("upnl", &self.upnl)?;
        match &self.info {
            None => {}
            Some(Input::Undefined) => state.skip_field("info")?,
            Some(value) => state.serialize_field("info", value)?,
        }
        if let Some(value) = &self.currencies {
            state.serialize_field("currencies", value)?;
        }
        if let Some(value) = &self.coins {
            state.serialize_field("coins", value)?;
        }
        state.end()
    }
}

/// Native initial state; loaded.balance remains false until an account read commits.
pub fn empty_balance() -> Balance {
    Balance {
        used: 0.0,
        free: 0.0,
        total: 0.0,
        upnl: 0.0,
        info: None,
        currencies: None,
        coins: None,
    }
}
