//! Canonical SDK Ticker contract, including source observation provenance.
use crate::retained_json::RetainedJsonInput as Input;
use serde::{Deserialize, ser::SerializeStruct};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Ticker {
    /// [last, mark, index] source receipt-clock milliseconds; not seconds; not snapshot receipt time.
    /// Null is unobserved; zero is valid.
    pub observed_at: Option<[Option<f64>; 3]>,
    pub id: String,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    /// Zero derivatives metadata may mean the venue did not publish the field.
    pub mark: f64,
    // WOO publishes null for a real market with no index price.
    #[ts(as = "f64")]
    pub index: Option<f64>,

    /// Exchange-published midpoint; absent is distinct from a numeric zero.
    pub mid_price: Option<f64>,
    pub percentage: f64,
    pub open_interest: f64,
    pub funding_rate: f64,
    pub funding_time: f64,
    pub volume: f64,
    pub quote_volume: f64,
    // Runtime extra emitted by Paradex fetchTickers from virtualClock.getCurrentTimeMs().
    // The generated TypeScript remains the source declaration projection.
    #[ts(skip)]
    pub timestamp: Option<f64>,

    #[ts(type = "unknown", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl serde::Serialize for Ticker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::INFO_RECORD, 17)?;
        if let Some(value) = &self.observed_at {
            state.serialize_field("observedAt", value)?;
        }
        state.serialize_field("id", &self.id)?;
        state.serialize_field("symbol", &self.symbol)?;
        state.serialize_field("bid", &self.bid)?;
        state.serialize_field("ask", &self.ask)?;
        state.serialize_field("last", &self.last)?;
        state.serialize_field("mark", &self.mark)?;
        state.serialize_field("index", &self.index)?;
        if let Some(value) = &self.mid_price {
            state.serialize_field("midPrice", value)?;
        }
        state.serialize_field("percentage", &self.percentage)?;
        state.serialize_field("openInterest", &self.open_interest)?;
        state.serialize_field("fundingRate", &self.funding_rate)?;
        state.serialize_field("fundingTime", &self.funding_time)?;
        state.serialize_field("volume", &self.volume)?;
        state.serialize_field("quoteVolume", &self.quote_volume)?;
        if let Some(value) = &self.timestamp {
            state.serialize_field("timestamp", value)?;
        }
        match &self.info {
            None => {}
            Some(Input::Undefined) => state.skip_field("info")?,
            Some(value) => state.serialize_field("info", value)?,
        }
        state.end()
    }
}

#[derive(serde::Deserialize)]
#[serde(remote = "Ticker")]
#[serde(rename_all = "camelCase")]
struct WireTicker {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    observed_at: Option<[Option<f64>; 3]>,
    id: String,
    symbol: String,
    bid: f64,
    ask: f64,
    last: f64,
    mark: f64,
    #[serde(deserialize_with = "Option::<f64>::deserialize")]
    index: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    mid_price: Option<f64>,
    percentage: f64,
    open_interest: f64,
    funding_rate: f64,
    funding_time: f64,
    volume: f64,
    quote_volume: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    timestamp: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::retained_json::deserialize_present_retained_info",
        skip_serializing_if = "Option::is_none"
    )]
    info: Option<crate::retained_json::RetainedJsonInput>,
}

impl<'de> serde::Deserialize<'de> for Ticker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InfoVisitor;
        impl<'de> serde::de::Visitor<'de> for InfoVisitor {
            type Value = Ticker;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a Ticker record")
            }
            fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Self::Value, D::Error> {
                WireTicker::deserialize(crate::strict_decode::object(d))
            }
        }
        deserializer.deserialize_newtype_struct(crate::model_field::INFO_RECORD, InfoVisitor)
    }
}
