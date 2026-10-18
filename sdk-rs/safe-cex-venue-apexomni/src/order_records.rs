//! Order wire records, timestamp union, and client-ID intersection.
use crate::interface::{ApexMarket, OrderSide, OrderStatus, OrderType, TimeInForce};
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

// ts: ISO8601
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Iso8601 {
    String(String),
    Number(f64),
}
impl<'de> serde::Deserialize<'de> for Iso8601 {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct TimestampVisitor;
        impl<'de> serde::de::Visitor<'de> for TimestampVisitor {
            type Value = Iso8601;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ISO8601 string or number")
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Iso8601::String(value))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Iso8601::String(value.to_owned()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Iso8601::Number(value as f64))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Iso8601::Number(value as f64))
            }
            fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Self::Value, E> {
                Ok(Iso8601::Number(value as f64))
            }
            fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Self::Value, E> {
                Ok(Iso8601::Number(value as f64))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Iso8601::Number(value))
            }
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| serde::Deserializer::deserialize_any(decoder, TimestampVisitor))
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for Iso8601 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { String(value), Number(value) });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CreateOrderKind {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_LIMIT")]
    StopLimit,
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TAKE_PROFIT_LIMIT")]
    TakeProfitLimit,
    #[serde(rename = "TAKE_PROFIT_MARKET")]
    TakeProfitMarket,
}
impl<'de> serde::Deserialize<'de> for CreateOrderKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "LIMIT" => Ok(Self::Limit),
            "MARKET" => Ok(Self::Market),
            "STOP_LIMIT" => Ok(Self::StopLimit),
            "STOP_MARKET" => Ok(Self::StopMarket),
            "TAKE_PROFIT_LIMIT" => Ok(Self::TakeProfitLimit),
            "TAKE_PROFIT_MARKET" => Ok(Self::TakeProfitMarket),
            _ => Err(serde::de::Error::custom("unsupported wire enum spelling")),
        }
    }
}
impl JsNumberOutput for CreateOrderKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market, StopLimit, StopMarket, TakeProfitLimit, TakeProfitMarket });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CreateTimeInForce {
    #[serde(rename = "GOOD_TIL_CANCEL")]
    GoodTilCancel,
    #[serde(rename = "FILL_OR_KILL")]
    FillOrKill,
    #[serde(rename = "IMMEDIATE_OR_CANCEL")]
    ImmediateOrCancel,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}
impl<'de> serde::Deserialize<'de> for CreateTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "GOOD_TIL_CANCEL" => Ok(Self::GoodTilCancel),
            "FILL_OR_KILL" => Ok(Self::FillOrKill),
            "IMMEDIATE_OR_CANCEL" => Ok(Self::ImmediateOrCancel),
            "POST_ONLY" => Ok(Self::PostOnly),
            _ => Err(serde::de::Error::custom("unsupported wire enum spelling")),
        }
    }
}
impl JsNumberOutput for CreateTimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { GoodTilCancel, FillOrKill, ImmediateOrCancel, PostOnly });
    }
}

// ts: WithPrice
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WithPrice {
    #[serde(rename = "humanPrice")]
    pub human_price: String,
    #[serde(rename = "humanQuoteAmount")]
    #[serde(skip_serializing)]
    pub human_quote_amount: Option<()>,
}
impl<'de> serde::Deserialize<'de> for WithPrice {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "humanPrice")]
            human_price: String,
            #[serde(rename = "humanQuoteAmount")]
            #[serde(default, deserialize_with = "undefined_only")]
            human_quote_amount: Option<()>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    human_price: fields.human_price,
                    human_quote_amount: fields.human_quote_amount,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WithPrice {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { human_price, human_quote_amount });
    }
}

// ts: OrderWithClientId
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderWithClientId {
    #[serde(rename = "positionId")]
    pub position_id: String,
    #[serde(rename = "humanSize")]
    pub human_size: String,
    #[serde(rename = "limitFee")]
    pub limit_fee: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "side")]
    pub side: OrderSide,
    #[serde(rename = "expirationIsoTimestamp")]
    pub expiration_iso_timestamp: String,
    #[serde(rename = "humanPrice")]
    pub human_price: String,
    #[serde(rename = "humanQuoteAmount")]
    #[serde(skip_serializing)]
    pub human_quote_amount: Option<()>,
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "nonce")]
    #[serde(skip_serializing)]
    pub nonce: Option<()>,
}
impl<'de> serde::Deserialize<'de> for OrderWithClientId {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "positionId")]
            position_id: String,
            #[serde(rename = "humanSize")]
            human_size: String,
            #[serde(rename = "limitFee")]
            limit_fee: String,
            #[serde(rename = "symbol")]
            symbol: String,
            #[serde(rename = "side")]
            side: OrderSide,
            #[serde(rename = "expirationIsoTimestamp")]
            expiration_iso_timestamp: String,
            #[serde(rename = "humanPrice")]
            human_price: String,
            #[serde(rename = "humanQuoteAmount")]
            #[serde(default, deserialize_with = "undefined_only")]
            human_quote_amount: Option<()>,
            #[serde(rename = "clientId")]
            client_id: String,
            #[serde(rename = "nonce")]
            #[serde(default, deserialize_with = "undefined_only")]
            nonce: Option<()>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    position_id: fields.position_id,
                    human_size: fields.human_size,
                    limit_fee: fields.limit_fee,
                    symbol: fields.symbol,
                    side: fields.side,
                    expiration_iso_timestamp: fields.expiration_iso_timestamp,
                    human_price: fields.human_price,
                    human_quote_amount: fields.human_quote_amount,
                    client_id: fields.client_id,
                    nonce: fields.nonce,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for OrderWithClientId {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { position_id, human_size, limit_fee, symbol, side, expiration_iso_timestamp, human_price, human_quote_amount, client_id, nonce });
    }
}

// ts: CreateOrderOptionsObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateOrderOptionsObject {
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "positionId")]
    pub position_id: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "side")]
    pub side: OrderSide,
    #[serde(rename = "type")]
    pub r#type: CreateOrderKind,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "limitFee")]
    pub limit_fee: String,
    #[serde(rename = "timeInForce")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<CreateTimeInForce>,
    #[serde(rename = "triggerPrice")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<String>,
    #[serde(rename = "trailingPercent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_percent: Option<String>,
    #[serde(rename = "reduceOnly")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "brokerId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broker_id: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(remote = "CreateOrderOptionsObject")]
struct CreateOrderOptionsObjectWire {
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    #[serde(rename = "positionId")]
    position_id: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: OrderSide,
    #[serde(rename = "type")]
    r#type: CreateOrderKind,
    #[serde(rename = "size")]
    size: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "limitFee")]
    limit_fee: String,
    #[serde(rename = "timeInForce")]
    #[serde(default, deserialize_with = "deserialize_present")]
    time_in_force: Option<CreateTimeInForce>,
    #[serde(rename = "triggerPrice")]
    #[serde(default, deserialize_with = "deserialize_present")]
    trigger_price: Option<String>,
    #[serde(rename = "trailingPercent")]
    #[serde(default, deserialize_with = "deserialize_present")]
    trailing_percent: Option<String>,
    #[serde(rename = "reduceOnly")]
    #[serde(default, deserialize_with = "deserialize_present")]
    reduce_only: Option<bool>,
    #[serde(rename = "brokerId")]
    #[serde(default, deserialize_with = "deserialize_present")]
    broker_id: Option<String>,
}
impl<'de> serde::Deserialize<'de> for CreateOrderOptionsObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                CreateOrderOptionsObjectWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for CreateOrderOptionsObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { client_order_id, position_id, symbol, side, r#type, size, price, limit_fee, time_in_force, trigger_price, trailing_percent, reduce_only, broker_id });
    }
}

// ts: OrderResponseObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderResponseObject {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "clientId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "symbol")]
    pub symbol: ApexMarket,
    #[serde(rename = "side")]
    pub side: OrderSide,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "triggerPrice")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<Option<String>>,
    #[serde(rename = "trailingPercent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_percent: Option<Option<String>>,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "remainingSize")]
    pub remaining_size: String,
    #[serde(rename = "type")]
    pub r#type: OrderType,
    #[serde(rename = "createdAt")]
    pub created_at: Iso8601,
    #[serde(rename = "unfillableAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfillable_at: Option<Option<Iso8601>>,
    #[serde(rename = "expiresAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Iso8601>,
    #[serde(rename = "status")]
    pub status: OrderStatus,
    #[serde(rename = "timeInForce")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "postOnly")]
    pub post_only: bool,
    #[serde(rename = "cancelReason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<Option<String>>,
}
#[derive(serde::Deserialize)]
#[serde(remote = "OrderResponseObject")]
struct OrderResponseObjectWire {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "clientId")]
    #[serde(default, deserialize_with = "deserialize_present")]
    client_id: Option<String>,
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "symbol")]
    symbol: ApexMarket,
    #[serde(rename = "side")]
    side: OrderSide,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "triggerPrice")]
    #[serde(default, deserialize_with = "deserialize_present")]
    trigger_price: Option<Option<String>>,
    #[serde(rename = "trailingPercent")]
    #[serde(default, deserialize_with = "deserialize_present")]
    trailing_percent: Option<Option<String>>,
    #[serde(rename = "size")]
    size: String,
    #[serde(rename = "remainingSize")]
    remaining_size: String,
    #[serde(rename = "type")]
    r#type: OrderType,
    #[serde(rename = "createdAt")]
    created_at: Iso8601,
    #[serde(rename = "unfillableAt")]
    #[serde(default, deserialize_with = "deserialize_present")]
    unfillable_at: Option<Option<Iso8601>>,
    #[serde(rename = "expiresAt")]
    #[serde(default, deserialize_with = "deserialize_present")]
    expires_at: Option<Iso8601>,
    #[serde(rename = "status")]
    status: OrderStatus,
    #[serde(rename = "timeInForce")]
    time_in_force: TimeInForce,
    #[serde(rename = "postOnly")]
    post_only: bool,
    #[serde(rename = "cancelReason")]
    #[serde(default, deserialize_with = "deserialize_present")]
    cancel_reason: Option<Option<String>>,
}
impl<'de> serde::Deserialize<'de> for OrderResponseObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                OrderResponseObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for OrderResponseObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, client_id, account_id, symbol, side, price, trigger_price, trailing_percent, size, remaining_size, r#type, created_at, unfillable_at, expires_at, status, time_in_force, post_only, cancel_reason });
    }
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
fn undefined_only<'de, D: serde::Deserializer<'de>>(_decoder: D) -> Result<Option<()>, D::Error> {
    Err(serde::de::Error::custom(
        "undefined-only property cannot be present in JSON",
    ))
}
