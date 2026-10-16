//! Wire declarations from bitunix.types.ts; decode through numeric_ingress.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use serde::{Deserialize, Serialize};

pub type GetPendingPositionsResponse = Vec<PendingPosition>;

#[derive(Debug, Clone, PartialEq)]
pub struct PendingPosition {
    pub symbol: String,
    pub position_id: String,
    pub qty: String,
    pub max_qty: Option<String>,
    pub entry_value: String,
    pub entry_price: Option<String>,
    pub avg_open_price: Option<String>,
    pub side: PendingPositionSide,
    pub margin_mode: PendingMarginMode,
    pub position_mode: PendingPositionMode,
    pub leverage: f64,
    pub fee: String,
    pub funding: String,
    pub realized_pnl: String,
    pub margin: String,
    pub unrealized_pnl: String,
    pub liq_price: String,
    pub margin_rate: Option<String>,
    pub ctime: f64,
    pub mtime: f64,
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PendingPosition")]
struct PendingPositionWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "positionId")]
    position_id: String,
    #[serde(rename = "qty")]
    qty: String,
    #[serde(rename = "maxQty")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    max_qty: Option<String>,
    #[serde(rename = "entryValue")]
    entry_value: String,
    #[serde(rename = "entryPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    entry_price: Option<String>,
    #[serde(rename = "avgOpenPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    avg_open_price: Option<String>,
    #[serde(rename = "side")]
    side: PendingPositionSide,
    #[serde(rename = "marginMode")]
    margin_mode: PendingMarginMode,
    #[serde(rename = "positionMode")]
    position_mode: PendingPositionMode,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "fee")]
    fee: String,
    #[serde(rename = "funding")]
    funding: String,
    #[serde(rename = "realizedPNL")]
    realized_pnl: String,
    #[serde(rename = "margin")]
    margin: String,
    #[serde(rename = "unrealizedPNL")]
    unrealized_pnl: String,
    #[serde(rename = "liqPrice")]
    liq_price: String,
    #[serde(rename = "marginRate")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    margin_rate: Option<String>,
    #[serde(rename = "ctime")]
    ctime: f64,
    #[serde(rename = "mtime")]
    mtime: f64,
}
impl Serialize for PendingPosition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PendingPositionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PendingPosition {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PendingPositionWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for PendingPosition {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, position_id, qty, max_qty, entry_value, entry_price, avg_open_price, side, margin_mode, position_mode, leverage, fee, funding, realized_pnl, margin, unrealized_pnl, liq_price, margin_rate, ctime, mtime });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PendingPositionSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl JsNumberOutput for PendingPositionSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell, Long, Short });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PendingMarginMode {
    #[serde(rename = "ISOLATION")]
    Isolation,
    #[serde(rename = "CROSS")]
    Cross,
}
impl JsNumberOutput for PendingMarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Isolation, Cross });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PendingPositionMode {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGED")]
    Hedged,
    #[serde(rename = "HEDGE")]
    Hedge,
}
impl JsNumberOutput for PendingPositionMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { OneWay, Hedged, Hedge });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KlineInterval {
    // 1min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "1min")]
    Value0,
    #[serde(rename = "mark_kline_1min")]
    MarkKline1min,
    // 3min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "3min")]
    Value2,
    #[serde(rename = "mark_kline_3min")]
    MarkKline3min,
    // 5min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "5min")]
    Value4,
    #[serde(rename = "mark_kline_5min")]
    MarkKline5min,
    // 15min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "15min")]
    Value6,
    #[serde(rename = "mark_kline_15min")]
    MarkKline15min,
    // 30min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "30min")]
    Value8,
    #[serde(rename = "mark_kline_30min")]
    MarkKline30min,
    // 60min: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "60min")]
    Value10,
    #[serde(rename = "mark_kline_60min")]
    MarkKline60min,
    #[serde(rename = "2h")]
    H2,
    #[serde(rename = "mark_kline_2h")]
    MarkKline2h,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "mark_kline_4h")]
    MarkKline4h,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "mark_kline_6h")]
    MarkKline6h,
    #[serde(rename = "8h")]
    H8,
    #[serde(rename = "mark_kline_8h")]
    MarkKline8h,
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "mark_kline_12h")]
    MarkKline12h,
    // 1day: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "1day")]
    Value22,
    #[serde(rename = "mark_kline_1day")]
    MarkKline1day,
    // 3day: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "3day")]
    Value24,
    #[serde(rename = "mark_kline_3day")]
    MarkKline3day,
    // 1week: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "1week")]
    Value26,
    #[serde(rename = "mark_kline_1week")]
    MarkKline1week,
    // 1month: digit-leading literal outside the standard interval-unit rule.
    #[serde(rename = "1month")]
    Value28,
    #[serde(rename = "mark_kline_1month")]
    MarkKline1month,
}
impl JsNumberOutput for KlineInterval {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value0, MarkKline1min, Value2, MarkKline3min, Value4, MarkKline5min, Value6, MarkKline15min, Value8, MarkKline30min, Value10, MarkKline60min, H2, MarkKline2h, H4, MarkKline4h, H6, MarkKline6h, H8, MarkKline8h, H12, MarkKline12h, Value22, MarkKline1day, Value24, MarkKline3day, Value26, MarkKline1week, Value28, MarkKline1month });
    }
}

impl<'de> Deserialize<'de> for PendingPositionSide {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            "LONG" => Ok(Self::Long),
            "SHORT" => Ok(Self::Short),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["BUY", "SELL", "LONG", "SHORT"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for PendingMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "ISOLATION" => Ok(Self::Isolation),
            "CROSS" => Ok(Self::Cross),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["ISOLATION", "CROSS"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for PendingPositionMode {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "ONE_WAY" => Ok(Self::OneWay),
            "HEDGED" => Ok(Self::Hedged),
            "HEDGE" => Ok(Self::Hedge),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["ONE_WAY", "HEDGED", "HEDGE"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for KlineInterval {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "1min" => Ok(Self::Value0),
            "mark_kline_1min" => Ok(Self::MarkKline1min),
            "3min" => Ok(Self::Value2),
            "mark_kline_3min" => Ok(Self::MarkKline3min),
            "5min" => Ok(Self::Value4),
            "mark_kline_5min" => Ok(Self::MarkKline5min),
            "15min" => Ok(Self::Value6),
            "mark_kline_15min" => Ok(Self::MarkKline15min),
            "30min" => Ok(Self::Value8),
            "mark_kline_30min" => Ok(Self::MarkKline30min),
            "60min" => Ok(Self::Value10),
            "mark_kline_60min" => Ok(Self::MarkKline60min),
            "2h" => Ok(Self::H2),
            "mark_kline_2h" => Ok(Self::MarkKline2h),
            "4h" => Ok(Self::H4),
            "mark_kline_4h" => Ok(Self::MarkKline4h),
            "6h" => Ok(Self::H6),
            "mark_kline_6h" => Ok(Self::MarkKline6h),
            "8h" => Ok(Self::H8),
            "mark_kline_8h" => Ok(Self::MarkKline8h),
            "12h" => Ok(Self::H12),
            "mark_kline_12h" => Ok(Self::MarkKline12h),
            "1day" => Ok(Self::Value22),
            "mark_kline_1day" => Ok(Self::MarkKline1day),
            "3day" => Ok(Self::Value24),
            "mark_kline_3day" => Ok(Self::MarkKline3day),
            "1week" => Ok(Self::Value26),
            "mark_kline_1week" => Ok(Self::MarkKline1week),
            "1month" => Ok(Self::Value28),
            "mark_kline_1month" => Ok(Self::MarkKline1month),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &[
                    "1min",
                    "mark_kline_1min",
                    "3min",
                    "mark_kline_3min",
                    "5min",
                    "mark_kline_5min",
                    "15min",
                    "mark_kline_15min",
                    "30min",
                    "mark_kline_30min",
                    "60min",
                    "mark_kline_60min",
                    "2h",
                    "mark_kline_2h",
                    "4h",
                    "mark_kline_4h",
                    "6h",
                    "mark_kline_6h",
                    "8h",
                    "mark_kline_8h",
                    "12h",
                    "mark_kline_12h",
                    "1day",
                    "mark_kline_1day",
                    "3day",
                    "mark_kline_3day",
                    "1week",
                    "mark_kline_1week",
                    "1month",
                    "mark_kline_1month",
                ],
            )),
        }
    }
}
