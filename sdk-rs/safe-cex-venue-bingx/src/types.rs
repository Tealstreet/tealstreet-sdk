//! Declared BingX wire types; not validators for unchecked Axios runtime data.
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BingxOrderStatus {
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "PARTIALLY_FILLED")]
    PartiallyFilled,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "WORKING")]
    Working,
    #[serde(rename = "EXPIRED")]
    Expired,
}
impl<'de> Deserialize<'de> for BingxOrderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "CANCELED" => Ok(Self::Canceled),
            "FILLED" => Ok(Self::Filled),
            "NEW" => Ok(Self::New),
            "OPEN" => Ok(Self::Open),
            "PARTIALLY_FILLED" => Ok(Self::PartiallyFilled),
            "PENDING" => Ok(Self::Pending),
            "WORKING" => Ok(Self::Working),
            "EXPIRED" => Ok(Self::Expired),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "CANCELED",
                    "FILLED",
                    "NEW",
                    "OPEN",
                    "PARTIALLY_FILLED",
                    "PENDING",
                    "WORKING",
                    "EXPIRED",
                ],
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BingxOrderStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Canceled, Filled, New, Open, PartiallyFilled, Pending, Working, Expired });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BingxMarginType {
    #[serde(rename = "CROSSED")]
    Crossed,
    #[serde(rename = "ISOLATED")]
    Isolated,
}
impl<'de> Deserialize<'de> for BingxMarginType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "CROSSED" => Ok(Self::Crossed),
            "ISOLATED" => Ok(Self::Isolated),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["CROSSED", "ISOLATED"],
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BingxMarginType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Crossed, Isolated });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BingxDualSidePosition {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
}
impl<'de> Deserialize<'de> for BingxDualSidePosition {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["true", "false"],
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BingxDualSidePosition {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { True, False });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderTradeUpdateEvent {
    #[serde(rename = "ORDER_TRADE_UPDATE")]
    OrderTradeUpdate,
}
impl<'de> Deserialize<'de> for OrderTradeUpdateEvent {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "ORDER_TRADE_UPDATE" => Ok(Self::OrderTradeUpdate),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["ORDER_TRADE_UPDATE"],
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderTradeUpdateEvent {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { OrderTradeUpdate });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BingxBalance {
    pub user_id: String,
    pub asset: String,
    pub balance: String,
    pub equity: String,
    pub unrealized_profit: String,
    pub realised_profit: String,
    pub available_margin: String,
    pub used_margin: String,
    pub freezed_margin: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "BingxBalance")]
struct BingxBalanceWire {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "asset")]
    asset: String,
    #[serde(rename = "balance")]
    balance: String,
    #[serde(rename = "equity")]
    equity: String,
    #[serde(rename = "unrealizedProfit")]
    unrealized_profit: String,
    #[serde(rename = "realisedProfit")]
    realised_profit: String,
    #[serde(rename = "availableMargin")]
    available_margin: String,
    #[serde(rename = "usedMargin")]
    used_margin: String,
    #[serde(rename = "freezedMargin")]
    freezed_margin: String,
}
impl Serialize for BingxBalance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BingxBalanceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BingxBalance {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BingxBalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BingxBalance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { user_id, asset, balance, equity, unrealized_profit, realised_profit, available_margin, used_margin, freezed_margin });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchBalanceResponse {
    pub code: f64,
    pub msg: String,
    pub data: Vec<BingxBalance>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchBalanceResponse")]
struct FetchBalanceResponseWire {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: Vec<BingxBalance>,
}
impl Serialize for FetchBalanceResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchBalanceResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchBalanceResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchBalanceResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchBalanceResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchLeverageResponseData {
    pub long_leverage: f64,
    pub short_leverage: f64,
    pub max_long_leverage: f64,
    pub max_short_leverage: f64,
    pub available_long_vol: String,
    pub available_short_vol: String,
    pub available_long_val: String,
    pub available_short_val: String,
    pub max_position_long_val: String,
    pub max_position_short_val: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchLeverageResponseData")]
struct FetchLeverageResponseDataWire {
    #[serde(rename = "longLeverage")]
    long_leverage: f64,
    #[serde(rename = "shortLeverage")]
    short_leverage: f64,
    #[serde(rename = "maxLongLeverage")]
    max_long_leverage: f64,
    #[serde(rename = "maxShortLeverage")]
    max_short_leverage: f64,
    #[serde(rename = "availableLongVol")]
    available_long_vol: String,
    #[serde(rename = "availableShortVol")]
    available_short_vol: String,
    #[serde(rename = "availableLongVal")]
    available_long_val: String,
    #[serde(rename = "availableShortVal")]
    available_short_val: String,
    #[serde(rename = "maxPositionLongVal")]
    max_position_long_val: String,
    #[serde(rename = "maxPositionShortVal")]
    max_position_short_val: String,
}
impl Serialize for FetchLeverageResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchLeverageResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchLeverageResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchLeverageResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchLeverageResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { long_leverage, short_leverage, max_long_leverage, max_short_leverage, available_long_vol, available_short_vol, available_long_val, available_short_val, max_position_long_val, max_position_short_val });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchLeverageResponse {
    pub code: f64,
    pub msg: String,
    pub data: FetchLeverageResponseData,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchLeverageResponse")]
struct FetchLeverageResponseWire {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: FetchLeverageResponseData,
}
impl Serialize for FetchLeverageResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchLeverageResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchLeverageResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchLeverageResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchLeverageResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchMarginModeResponseData {
    pub margin_type: BingxMarginType,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchMarginModeResponseData")]
struct FetchMarginModeResponseDataWire {
    #[serde(rename = "marginType")]
    margin_type: BingxMarginType,
}
impl Serialize for FetchMarginModeResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchMarginModeResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchMarginModeResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchMarginModeResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchMarginModeResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { margin_type });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchMarginModeResponse {
    pub code: f64,
    pub msg: String,
    pub data: FetchMarginModeResponseData,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchMarginModeResponse")]
struct FetchMarginModeResponseWire {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: FetchMarginModeResponseData,
}
impl Serialize for FetchMarginModeResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchMarginModeResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchMarginModeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchMarginModeResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchMarginModeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchHedgeModeResponseData {
    pub dual_side_position: BingxDualSidePosition,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchHedgeModeResponseData")]
struct FetchHedgeModeResponseDataWire {
    #[serde(rename = "dualSidePosition")]
    dual_side_position: BingxDualSidePosition,
}
impl Serialize for FetchHedgeModeResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchHedgeModeResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchHedgeModeResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchHedgeModeResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchHedgeModeResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { dual_side_position });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchHedgeModeResponse {
    pub code: f64,
    pub msg: String,
    pub time_stamp: f64,
    pub data: FetchHedgeModeResponseData,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchHedgeModeResponse")]
struct FetchHedgeModeResponseWire {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "timeStamp")]
    time_stamp: f64,
    #[serde(rename = "data")]
    data: FetchHedgeModeResponseData,
}
impl Serialize for FetchHedgeModeResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchHedgeModeResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchHedgeModeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchHedgeModeResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchHedgeModeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, time_stamp, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderTradeUpdateData {
    pub s: String,
    pub i: String,
    pub side: String,
    pub o: String,
    pub q: String,
    pub p: String,
    pub sp: String,
    pub ap: String,
    pub x: String,
    pub status: BingxOrderStatus,
    pub commission_asset: String,
    pub n: String,
    pub trade_time: f64,
    pub wt: String,
    pub ps: String,
    pub rp: String,
    pub z: String,
    pub sg: String,
    pub ti: f64,
    pub reduce_only: Option<bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderTradeUpdateData")]
struct OrderTradeUpdateDataWire {
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "S")]
    side: String,
    #[serde(rename = "o")]
    o: String,
    #[serde(rename = "q")]
    q: String,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "sp")]
    sp: String,
    #[serde(rename = "ap")]
    ap: String,
    #[serde(rename = "x")]
    x: String,
    #[serde(rename = "X")]
    status: BingxOrderStatus,
    #[serde(rename = "N")]
    commission_asset: String,
    #[serde(rename = "n")]
    n: String,
    #[serde(rename = "T")]
    trade_time: f64,
    #[serde(rename = "wt")]
    wt: String,
    #[serde(rename = "ps")]
    ps: String,
    #[serde(rename = "rp")]
    rp: String,
    #[serde(rename = "z")]
    z: String,
    #[serde(rename = "sg")]
    sg: String,
    #[serde(rename = "ti")]
    ti: f64,
    #[serde(rename = "R")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    reduce_only: Option<bool>,
}
impl Serialize for OrderTradeUpdateData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderTradeUpdateDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderTradeUpdateData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                OrderTradeUpdateDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderTradeUpdateData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { s, i, side, o, q, p, sp, ap, x, status, commission_asset, n, trade_time, wt, ps, rp, z, sg, ti, reduce_only });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderTradeUpdate {
    pub e: OrderTradeUpdateEvent,
    pub event_time: f64,
    pub o: OrderTradeUpdateData,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderTradeUpdate")]
struct OrderTradeUpdateWire {
    #[serde(rename = "e")]
    e: OrderTradeUpdateEvent,
    #[serde(rename = "E")]
    event_time: f64,
    #[serde(rename = "o")]
    o: OrderTradeUpdateData,
}
impl Serialize for OrderTradeUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderTradeUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderTradeUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                OrderTradeUpdateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderTradeUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, event_time, o });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchPositionsResponseData {
    pub symbol: String,
    pub position_id: String,
    pub position_side: String,
    pub isolated: bool,
    pub position_amt: String,
    pub available_amt: String,
    pub unrealized_profit: String,
    pub realised_profit: String,
    pub initial_margin: String,
    pub margin: String,
    pub avg_price: String,
    pub liquidation_price: f64,
    pub leverage: f64,
    pub position_value: String,
    pub mark_price: String,
    pub risk_rate: String,
    pub max_margin_reduction: String,
    pub pnl_ratio: String,
    pub update_time: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchPositionsResponseData")]
struct FetchPositionsResponseDataWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "positionId")]
    position_id: String,
    #[serde(rename = "positionSide")]
    position_side: String,
    #[serde(rename = "isolated")]
    isolated: bool,
    #[serde(rename = "positionAmt")]
    position_amt: String,
    #[serde(rename = "availableAmt")]
    available_amt: String,
    #[serde(rename = "unrealizedProfit")]
    unrealized_profit: String,
    #[serde(rename = "realisedProfit")]
    realised_profit: String,
    #[serde(rename = "initialMargin")]
    initial_margin: String,
    #[serde(rename = "Margin")]
    margin: String,
    #[serde(rename = "avgPrice")]
    avg_price: String,
    #[serde(rename = "liquidationPrice")]
    liquidation_price: f64,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "positionValue")]
    position_value: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "riskRate")]
    risk_rate: String,
    #[serde(rename = "maxMarginReduction")]
    max_margin_reduction: String,
    #[serde(rename = "pnlRatio")]
    pnl_ratio: String,
    #[serde(rename = "updateTime")]
    update_time: f64,
}
impl Serialize for FetchPositionsResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchPositionsResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchPositionsResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchPositionsResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchPositionsResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, position_id, position_side, isolated, position_amt, available_amt, unrealized_profit, realised_profit, initial_margin, margin, avg_price, liquidation_price, leverage, position_value, mark_price, risk_rate, max_margin_reduction, pnl_ratio, update_time });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchPositionsResponse {
    pub code: f64,
    pub msg: String,
    pub data: Vec<FetchPositionsResponseData>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchPositionsResponse")]
struct FetchPositionsResponseWire {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: Vec<FetchPositionsResponseData>,
}
impl Serialize for FetchPositionsResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FetchPositionsResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FetchPositionsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                FetchPositionsResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FetchPositionsResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}
