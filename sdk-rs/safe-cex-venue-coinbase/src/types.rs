//! Wire shapes from coinbase.types.ts; maintain alongside the TypeScript source.
use serde::{Deserialize, Serialize};

// Missing optional keys are omitted; present null is not a string/object/bool.
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketResponseProductsItemFcmTradingSessionDetails {
    pub open_time: String,
    pub close_time: String,
    pub is_session_open: String,
}

#[derive(Deserialize)]
#[serde(remote = "MarketResponseProductsItemFcmTradingSessionDetails")]
struct MarketResponseProductsItemFcmTradingSessionDetailsWire {
    open_time: String,
    close_time: String,
    is_session_open: String,
}
impl<'de> Deserialize<'de> for MarketResponseProductsItemFcmTradingSessionDetails {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MarketResponseProductsItemFcmTradingSessionDetailsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for MarketResponseProductsItemFcmTradingSessionDetails
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { open_time, close_time, is_session_open });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for MarketResponseProductsItemFcmTradingSessionDetails
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketResponseProductsItemFutureProductDetailsPerpetualDetails {
    pub funding_rate: String,
    pub open_interest: String,
    pub funding_time: String,
}

#[derive(Deserialize)]
#[serde(remote = "MarketResponseProductsItemFutureProductDetailsPerpetualDetails")]
struct MarketResponseProductsItemFutureProductDetailsPerpetualDetailsWire {
    funding_rate: String,
    open_interest: String,
    funding_time: String,
}
impl<'de> Deserialize<'de> for MarketResponseProductsItemFutureProductDetailsPerpetualDetails {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MarketResponseProductsItemFutureProductDetailsPerpetualDetailsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for MarketResponseProductsItemFutureProductDetailsPerpetualDetails
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { funding_rate, open_interest, funding_time });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for MarketResponseProductsItemFutureProductDetailsPerpetualDetails
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketResponseProductsItemFutureProductDetails {
    pub venue: String,
    pub contract_expiry_type: String,
    pub contract_expiry: String,
    pub contract_code: String,
    pub contract_expiry_timezone: String,
    pub contract_root_unit: String,
    pub group_short_description: String,
    pub contract_size: String,
    pub perpetual_details: MarketResponseProductsItemFutureProductDetailsPerpetualDetails,
    pub group_description: String,
    pub risk_managed_by: String,
    pub contract_display_name: String,
}

#[derive(Deserialize)]
#[serde(remote = "MarketResponseProductsItemFutureProductDetails")]
struct MarketResponseProductsItemFutureProductDetailsWire {
    venue: String,
    contract_expiry_type: String,
    contract_expiry: String,
    contract_code: String,
    contract_expiry_timezone: String,
    contract_root_unit: String,
    group_short_description: String,
    contract_size: String,
    perpetual_details: MarketResponseProductsItemFutureProductDetailsPerpetualDetails,
    group_description: String,
    risk_managed_by: String,
    contract_display_name: String,
}
impl<'de> Deserialize<'de> for MarketResponseProductsItemFutureProductDetails {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MarketResponseProductsItemFutureProductDetailsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for MarketResponseProductsItemFutureProductDetails
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { venue, contract_expiry_type, contract_expiry, contract_code, contract_expiry_timezone, contract_root_unit, group_short_description, contract_size, perpetual_details, group_description, risk_managed_by, contract_display_name });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for MarketResponseProductsItemFutureProductDetails
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketResponseProductsItem {
    pub quote_name: String,
    pub base_name: String,
    pub cancel_only: bool,
    pub volume_percentage_change_24h: String,
    pub mid_market_price: String,
    pub base_display_symbol: String,
    pub quote_display_symbol: String,
    pub price_percentage_change_24h: String,
    pub quote_increment: String,
    pub post_only: bool,
    pub limit_only: bool,
    pub base_max_size: String,
    pub auction_mode: bool,
    pub price: String,
    pub product_id: String,
    pub alias: String,
    pub quote_currency_id: String,
    pub r#new: bool,
    pub trading_disabled: bool,
    pub fcm_trading_session_details: MarketResponseProductsItemFcmTradingSessionDetails,
    pub alias_to: Vec<String>,
    pub is_disabled: bool,
    pub base_currency_id: String,
    pub price_increment: String,
    pub watched: bool,
    pub quote_min_size: String,
    pub product_type: String,
    pub volume_24h: String,
    pub future_product_details: MarketResponseProductsItemFutureProductDetails,
    pub base_increment: String,
    pub base_min_size: String,
    pub view_only: bool,
    pub quote_max_size: String,
    pub status: String,
}

#[derive(Deserialize)]
#[serde(remote = "MarketResponseProductsItem")]
struct MarketResponseProductsItemWire {
    quote_name: String,
    base_name: String,
    cancel_only: bool,
    volume_percentage_change_24h: String,
    mid_market_price: String,
    base_display_symbol: String,
    quote_display_symbol: String,
    price_percentage_change_24h: String,
    quote_increment: String,
    post_only: bool,
    limit_only: bool,
    base_max_size: String,
    auction_mode: bool,
    price: String,
    product_id: String,
    alias: String,
    quote_currency_id: String,
    r#new: bool,
    trading_disabled: bool,
    fcm_trading_session_details: MarketResponseProductsItemFcmTradingSessionDetails,
    alias_to: Vec<String>,
    is_disabled: bool,
    base_currency_id: String,
    price_increment: String,
    watched: bool,
    quote_min_size: String,
    product_type: String,
    volume_24h: String,
    future_product_details: MarketResponseProductsItemFutureProductDetails,
    base_increment: String,
    base_min_size: String,
    view_only: bool,
    quote_max_size: String,
    status: String,
}
impl<'de> Deserialize<'de> for MarketResponseProductsItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MarketResponseProductsItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarketResponseProductsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { quote_name, base_name, cancel_only, volume_percentage_change_24h, mid_market_price, base_display_symbol, quote_display_symbol, price_percentage_change_24h, quote_increment, post_only, limit_only, base_max_size, auction_mode, price, product_id, alias, quote_currency_id, r#new, trading_disabled, fcm_trading_session_details, alias_to, is_disabled, base_currency_id, price_increment, watched, quote_min_size, product_type, volume_24h, future_product_details, base_increment, base_min_size, view_only, quote_max_size, status });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for MarketResponseProductsItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketResponse {
    pub num_products: f64,
    pub products: Vec<MarketResponseProductsItem>,
}

#[derive(Deserialize)]
#[serde(remote = "MarketResponse")]
struct MarketResponseWire {
    num_products: f64,
    products: Vec<MarketResponseProductsItem>,
}
impl<'de> Deserialize<'de> for MarketResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MarketResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarketResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { num_products, products });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for MarketResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CandlesResponseCandlesItem {
    pub start: String,
    pub low: String,
    pub high: String,
    pub open: String,
    pub close: String,
    pub volume: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for CandlesResponseCandlesItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            start: String,
            low: String,
            high: String,
            open: String,
            close: String,
            volume: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            start: value.start,
            low: value.low,
            high: value.high,
            open: value.open,
            close: value.close,
            volume: value.volume,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CandlesResponse {
    pub candles: Vec<CandlesResponseCandlesItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for CandlesResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            candles: Vec<CandlesResponseCandlesItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            candles: value.candles,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListAccountsResponseItemAvailableBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListAccountsResponseItemAvailableBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListAccountsResponseItemHold {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListAccountsResponseItemHold {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListAccountsResponseItem {
    pub uuid: String,
    pub name: String,
    pub currency: String,
    pub available_balance: ListAccountsResponseItemAvailableBalance,
    pub r#default: bool,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: String,
    pub r#type: String,
    pub ready: bool,
    pub hold: ListAccountsResponseItemHold,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListAccountsResponseItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            uuid: String,
            name: String,
            currency: String,
            available_balance: ListAccountsResponseItemAvailableBalance,
            r#default: bool,
            active: bool,
            created_at: String,
            updated_at: String,
            deleted_at: String,
            r#type: String,
            ready: bool,
            hold: ListAccountsResponseItemHold,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            uuid: value.uuid,
            name: value.name,
            currency: value.currency,
            available_balance: value.available_balance,
            r#default: value.r#default,
            active: value.active,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            r#type: value.r#type,
            ready: value.ready,
            hold: value.hold,
        })
    }
}

pub type ListAccountsResponse = Vec<ListAccountsResponseItem>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPortfoliosResponsePortfoliosItem {
    pub name: String,
    pub uuid: String,
    pub r#type: String,
    pub deleted: bool,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListPortfoliosResponsePortfoliosItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            name: String,
            uuid: String,
            r#type: String,
            deleted: bool,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            name: value.name,
            uuid: value.uuid,
            r#type: value.r#type,
            deleted: value.deleted,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPortfoliosResponse {
    pub portfolios: Vec<ListPortfoliosResponsePortfoliosItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListPortfoliosResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            portfolios: Vec<ListPortfoliosResponsePortfoliosItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            portfolios: value.portfolios,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemEntryVwap {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemEntryVwap
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemVwap {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemVwap
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListPositionsIntxResponsePositionsItemPositionSide {
    #[serde(rename = "POSITION_SIDE_LONG")]
    PositionSideLong,
    #[serde(rename = "POSITION_SIDE_SHORT")]
    PositionSideShort,
}
impl<'de> Deserialize<'de> for ListPositionsIntxResponsePositionsItemPositionSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "POSITION_SIDE_LONG")]
            PositionSideLong,
            #[serde(rename = "POSITION_SIDE_SHORT")]
            PositionSideShort,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::PositionSideLong => Ok(Self::PositionSideLong),
            Wire::PositionSideShort => Ok(Self::PositionSideShort),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemUnrealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemMarkPrice {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemMarkPrice
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemLiquidationPrice {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemLiquidationPrice
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemImNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemImNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemMmNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemMmNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPositionNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPositionNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItemPortfolioSummary {
    pub portfolio_uuid: String,
    pub collateral: String,
    pub position_notional: String,
    pub open_position_notional: String,
    pub pending_fees: String,
    pub borrow: String,
    pub accrued_interest: String,
    pub rolling_debt: String,
    pub portfolio_initial_margin: String,
    pub portfolio_im_notional:
        ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional,
    pub portfolio_maintenance_margin: String,
    pub portfolio_mm_notional:
        ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional,
    pub liquidation_percentage: String,
    pub liquidation_buffer: String,
    pub margin_type: String,
    pub margin_flags: String,
    pub liquidation_status: String,
    pub unrealized_pnl: ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl,
    pub buying_power: ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower,
    pub total_balance: ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance,
    pub max_withdrawal: ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItemPortfolioSummary
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            portfolio_uuid: String,
            collateral: String,
            position_notional: String,
            open_position_notional: String,
            pending_fees: String,
            borrow: String,
            accrued_interest: String,
            rolling_debt: String,
            portfolio_initial_margin: String,
            portfolio_im_notional:
                ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional,
            portfolio_maintenance_margin: String,
            portfolio_mm_notional:
                ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional,
            liquidation_percentage: String,
            liquidation_buffer: String,
            margin_type: String,
            margin_flags: String,
            liquidation_status: String,
            unrealized_pnl: ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl,
            buying_power: ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower,
            total_balance: ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance,
            max_withdrawal: ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            portfolio_uuid: value.portfolio_uuid,
            collateral: value.collateral,
            position_notional: value.position_notional,
            open_position_notional: value.open_position_notional,
            pending_fees: value.pending_fees,
            borrow: value.borrow,
            accrued_interest: value.accrued_interest,
            rolling_debt: value.rolling_debt,
            portfolio_initial_margin: value.portfolio_initial_margin,
            portfolio_im_notional: value.portfolio_im_notional,
            portfolio_maintenance_margin: value.portfolio_maintenance_margin,
            portfolio_mm_notional: value.portfolio_mm_notional,
            liquidation_percentage: value.liquidation_percentage,
            liquidation_buffer: value.liquidation_buffer,
            margin_type: value.margin_type,
            margin_flags: value.margin_flags,
            liquidation_status: value.liquidation_status,
            unrealized_pnl: value.unrealized_pnl,
            buying_power: value.buying_power,
            total_balance: value.total_balance,
            max_withdrawal: value.max_withdrawal,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponsePositionsItem {
    pub product_id: String,
    pub product_uuid: String,
    pub symbol: String,
    pub entry_vwap: ListPositionsIntxResponsePositionsItemEntryVwap,
    pub vwap: ListPositionsIntxResponsePositionsItemVwap,
    pub position_side: ListPositionsIntxResponsePositionsItemPositionSide,
    pub net_size: String,
    pub buy_order_size: String,
    pub sell_order_size: String,
    pub im_contribution: String,
    pub unrealized_pnl: ListPositionsIntxResponsePositionsItemUnrealizedPnl,
    pub mark_price: ListPositionsIntxResponsePositionsItemMarkPrice,
    pub liquidation_price: ListPositionsIntxResponsePositionsItemLiquidationPrice,
    pub leverage: String,
    pub im_notional: ListPositionsIntxResponsePositionsItemImNotional,
    pub mm_notional: ListPositionsIntxResponsePositionsItemMmNotional,
    pub position_notional: ListPositionsIntxResponsePositionsItemPositionNotional,
    pub margin_type: String,
    pub liquidation_buffer: String,
    pub liquidation_percentage: String,
    pub portfolio_summary: ListPositionsIntxResponsePositionsItemPortfolioSummary,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListPositionsIntxResponsePositionsItem
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            product_id: String,
            product_uuid: String,
            symbol: String,
            entry_vwap: ListPositionsIntxResponsePositionsItemEntryVwap,
            vwap: ListPositionsIntxResponsePositionsItemVwap,
            position_side: ListPositionsIntxResponsePositionsItemPositionSide,
            net_size: String,
            buy_order_size: String,
            sell_order_size: String,
            im_contribution: String,
            unrealized_pnl: ListPositionsIntxResponsePositionsItemUnrealizedPnl,
            mark_price: ListPositionsIntxResponsePositionsItemMarkPrice,
            liquidation_price: ListPositionsIntxResponsePositionsItemLiquidationPrice,
            leverage: String,
            im_notional: ListPositionsIntxResponsePositionsItemImNotional,
            mm_notional: ListPositionsIntxResponsePositionsItemMmNotional,
            position_notional: ListPositionsIntxResponsePositionsItemPositionNotional,
            margin_type: String,
            liquidation_buffer: String,
            liquidation_percentage: String,
            portfolio_summary: ListPositionsIntxResponsePositionsItemPortfolioSummary,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            product_id: value.product_id,
            product_uuid: value.product_uuid,
            symbol: value.symbol,
            entry_vwap: value.entry_vwap,
            vwap: value.vwap,
            position_side: value.position_side,
            net_size: value.net_size,
            buy_order_size: value.buy_order_size,
            sell_order_size: value.sell_order_size,
            im_contribution: value.im_contribution,
            unrealized_pnl: value.unrealized_pnl,
            mark_price: value.mark_price,
            liquidation_price: value.liquidation_price,
            leverage: value.leverage,
            im_notional: value.im_notional,
            mm_notional: value.mm_notional,
            position_notional: value.position_notional,
            margin_type: value.margin_type,
            liquidation_buffer: value.liquidation_buffer,
            liquidation_percentage: value.liquidation_percentage,
            portfolio_summary: value.portfolio_summary,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsIntxResponse {
    pub positions: Vec<ListPositionsIntxResponsePositionsItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListPositionsIntxResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            positions: Vec<ListPositionsIntxResponsePositionsItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            positions: value.positions,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListPositionsCfmResponsePositionsItemSide {
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl<'de> Deserialize<'de> for ListPositionsCfmResponsePositionsItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "LONG")]
            Long,
            #[serde(rename = "SHORT")]
            Short,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Long => Ok(Self::Long),
            Wire::Short => Ok(Self::Short),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsCfmResponsePositionsItem {
    pub product_id: String,
    pub expiration_time: String,
    pub side: ListPositionsCfmResponsePositionsItemSide,
    pub number_of_contracts: String,
    pub current_price: String,
    pub avg_entry_price: String,
    pub unrealized_pnl: String,
    pub daily_realized_pnl: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListPositionsCfmResponsePositionsItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            product_id: String,
            expiration_time: String,
            side: ListPositionsCfmResponsePositionsItemSide,
            number_of_contracts: String,
            current_price: String,
            avg_entry_price: String,
            unrealized_pnl: String,
            daily_realized_pnl: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            product_id: value.product_id,
            expiration_time: value.expiration_time,
            side: value.side,
            number_of_contracts: value.number_of_contracts,
            current_price: value.current_price,
            avg_entry_price: value.avg_entry_price,
            unrealized_pnl: value.unrealized_pnl,
            daily_realized_pnl: value.daily_realized_pnl,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListPositionsCfmResponse {
    pub positions: Vec<ListPositionsCfmResponsePositionsItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListPositionsCfmResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            positions: Vec<ListPositionsCfmResponsePositionsItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            positions: value.positions,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CandleEventType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "update")]
    Update,
}
impl<'de> Deserialize<'de> for CandleEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "snapshot")]
            Snapshot,
            #[serde(rename = "update")]
            Update,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Snapshot => Ok(Self::Snapshot),
            Wire::Update => Ok(Self::Update),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CandleEventCandlesItem {
    pub start: String,
    pub high: String,
    pub low: String,
    pub open: String,
    pub close: String,
    pub volume: String,
    pub product_id: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for CandleEventCandlesItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            start: String,
            high: String,
            low: String,
            open: String,
            close: String,
            volume: String,
            product_id: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            start: value.start,
            high: value.high,
            low: value.low,
            open: value.open,
            close: value.close,
            volume: value.volume,
            product_id: value.product_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CandleEvent {
    pub r#type: CandleEventType,
    pub candles: Vec<CandleEventCandlesItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for CandleEvent {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: CandleEventType,
            candles: Vec<CandleEventCandlesItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            candles: value.candles,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TradeEventType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "update")]
    Update,
}
impl<'de> Deserialize<'de> for TradeEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "snapshot")]
            Snapshot,
            #[serde(rename = "update")]
            Update,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Snapshot => Ok(Self::Snapshot),
            Wire::Update => Ok(Self::Update),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TradeEventTradesItemSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for TradeEventTradesItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "BUY")]
            Buy,
            #[serde(rename = "SELL")]
            Sell,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Buy => Ok(Self::Buy),
            Wire::Sell => Ok(Self::Sell),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct TradeEventTradesItem {
    pub product_id: String,
    pub trade_id: String,
    pub price: String,
    pub size: String,
    pub time: String,
    pub side: TradeEventTradesItemSide,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for TradeEventTradesItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            product_id: String,
            trade_id: String,
            price: String,
            size: String,
            time: String,
            side: TradeEventTradesItemSide,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            product_id: value.product_id,
            trade_id: value.trade_id,
            price: value.price,
            size: value.size,
            time: value.time,
            side: value.side,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct TradeEvent {
    pub r#type: TradeEventType,
    pub trades: Vec<TradeEventTradesItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for TradeEvent {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: TradeEventType,
            trades: Vec<TradeEventTradesItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            trades: value.trades,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TickerEventType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "update")]
    Update,
}
impl<'de> Deserialize<'de> for TickerEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "snapshot")]
            Snapshot,
            #[serde(rename = "update")]
            Update,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Snapshot => Ok(Self::Snapshot),
            Wire::Update => Ok(Self::Update),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TickerEventTickersItemType {
    #[serde(rename = "ticker")]
    Ticker,
}
impl<'de> Deserialize<'de> for TickerEventTickersItemType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "ticker")]
            Ticker,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Ticker => Ok(Self::Ticker),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct TickerEventTickersItem {
    pub r#type: TickerEventTickersItemType,
    pub product_id: String,
    pub price: String,
    pub volume_24_h: String,
    pub low_24_h: String,
    pub high_24_h: String,
    pub low_52_w: String,
    pub high_52_w: String,
    pub price_percent_chg_24_h: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for TickerEventTickersItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: TickerEventTickersItemType,
            product_id: String,
            price: String,
            volume_24_h: String,
            low_24_h: String,
            high_24_h: String,
            low_52_w: String,
            high_52_w: String,
            price_percent_chg_24_h: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            product_id: value.product_id,
            price: value.price,
            volume_24_h: value.volume_24_h,
            low_24_h: value.low_24_h,
            high_24_h: value.high_24_h,
            low_52_w: value.low_52_w,
            high_52_w: value.high_52_w,
            price_percent_chg_24_h: value.price_percent_chg_24_h,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct TickerEvent {
    pub r#type: TickerEventType,
    pub tickers: Vec<TickerEventTickersItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for TickerEvent {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: TickerEventType,
            tickers: Vec<TickerEventTickersItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            tickers: value.tickers,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Level2EventType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "update")]
    Update,
}
impl<'de> Deserialize<'de> for Level2EventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "snapshot")]
            Snapshot,
            #[serde(rename = "update")]
            Update,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Snapshot => Ok(Self::Snapshot),
            Wire::Update => Ok(Self::Update),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Level2EventUpdatesItemSide {
    #[serde(rename = "bid")]
    Bid,
    #[serde(rename = "offer")]
    Offer,
}
impl<'de> Deserialize<'de> for Level2EventUpdatesItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "bid")]
            Bid,
            #[serde(rename = "offer")]
            Offer,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Bid => Ok(Self::Bid),
            Wire::Offer => Ok(Self::Offer),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct Level2EventUpdatesItem {
    pub side: Level2EventUpdatesItemSide,
    pub event_time: String,
    pub price_level: String,
    pub new_quantity: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for Level2EventUpdatesItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            side: Level2EventUpdatesItemSide,
            event_time: String,
            price_level: String,
            new_quantity: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            side: value.side,
            event_time: value.event_time,
            price_level: value.price_level,
            new_quantity: value.new_quantity,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct Level2Event {
    pub r#type: Level2EventType,
    pub product_id: String,
    pub updates: Vec<Level2EventUpdatesItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for Level2Event {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: Level2EventType,
            product_id: String,
            updates: Vec<Level2EventUpdatesItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            product_id: value.product_id,
            updates: value.updates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntxInstrumentsResponseItemQuote {
    pub best_bid_price: String,
    pub best_bid_size: String,
    pub best_ask_price: String,
    pub best_ask_size: String,
    pub trade_price: String,
    pub trade_qty: String,
    pub index_price: String,
    pub mark_price: String,
    pub settlement_price: String,
    pub limit_up: String,
    pub limit_down: String,
    pub predicted_funding: String,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "IntxInstrumentsResponseItemQuote")]
struct IntxInstrumentsResponseItemQuoteWire {
    best_bid_price: String,
    best_bid_size: String,
    best_ask_price: String,
    best_ask_size: String,
    trade_price: String,
    trade_qty: String,
    index_price: String,
    mark_price: String,
    settlement_price: String,
    limit_up: String,
    limit_down: String,
    predicted_funding: String,
    timestamp: String,
}
impl<'de> Deserialize<'de> for IntxInstrumentsResponseItemQuote {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                IntxInstrumentsResponseItemQuoteWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for IntxInstrumentsResponseItemQuote {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { best_bid_price, best_bid_size, best_ask_price, best_ask_size, trade_price, trade_qty, index_price, mark_price, settlement_price, limit_up, limit_down, predicted_funding, timestamp });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for IntxInstrumentsResponseItemQuote {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntxInstrumentsResponseItem {
    pub instrument_id: String,
    pub instrument_uuid: String,
    pub symbol: String,
    pub r#type: String,
    pub base_asset_id: String,
    pub base_asset_uuid: String,
    pub base_asset_name: String,
    pub quote_asset_id: String,
    pub quote_asset_uuid: String,
    pub quote_asset_name: String,
    pub base_increment: String,
    pub quote_increment: String,
    pub price_band_percent: f64,
    pub market_order_percent: f64,
    pub qty_24hr: String,
    pub notional_24hr: String,
    pub avg_daily_qty: String,
    pub avg_daily_notional: String,
    pub previous_day_qty: String,
    pub open_interest: String,
    pub position_limit_qty: String,
    pub position_limit_adq_pct: f64,
    pub replacement_cost: String,
    pub base_imf: f64,
    pub min_notional_value: String,
    pub funding_interval: String,
    pub trading_state: String,
    pub quote: IntxInstrumentsResponseItemQuote,
}

#[derive(Deserialize)]
#[serde(remote = "IntxInstrumentsResponseItem")]
struct IntxInstrumentsResponseItemWire {
    instrument_id: String,
    instrument_uuid: String,
    symbol: String,
    r#type: String,
    base_asset_id: String,
    base_asset_uuid: String,
    base_asset_name: String,
    quote_asset_id: String,
    quote_asset_uuid: String,
    quote_asset_name: String,
    base_increment: String,
    quote_increment: String,
    price_band_percent: f64,
    market_order_percent: f64,
    qty_24hr: String,
    notional_24hr: String,
    avg_daily_qty: String,
    avg_daily_notional: String,
    previous_day_qty: String,
    open_interest: String,
    position_limit_qty: String,
    position_limit_adq_pct: f64,
    replacement_cost: String,
    base_imf: f64,
    min_notional_value: String,
    funding_interval: String,
    trading_state: String,
    quote: IntxInstrumentsResponseItemQuote,
}
impl<'de> Deserialize<'de> for IntxInstrumentsResponseItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                IntxInstrumentsResponseItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for IntxInstrumentsResponseItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { instrument_id, instrument_uuid, symbol, r#type, base_asset_id, base_asset_uuid, base_asset_name, quote_asset_id, quote_asset_uuid, quote_asset_name, base_increment, quote_increment, price_band_percent, market_order_percent, qty_24hr, notional_24hr, avg_daily_qty, avg_daily_notional, previous_day_qty, open_interest, position_limit_qty, position_limit_adq_pct, replacement_cost, base_imf, min_notional_value, funding_interval, trading_state, quote });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for IntxInstrumentsResponseItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

pub type IntxInstrumentsResponse = Vec<IntxInstrumentsResponseItem>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryPortfolioImNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseSummaryPortfolioImNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryPortfolioMmNotional {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseSummaryPortfolioMmNotional
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseSummaryUnrealizedPnl {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryBuyingPower {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseSummaryBuyingPower {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryTotalBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseSummaryTotalBalance {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummaryMaxWithdrawal {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseSummaryMaxWithdrawal {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseSummary {
    pub portfolio_uuid: String,
    pub collateral: String,
    pub position_notional: String,
    pub open_position_notional: String,
    pub pending_fees: String,
    pub borrow: String,
    pub accrued_interest: String,
    pub rolling_debt: String,
    pub portfolio_initial_margin: String,
    pub portfolio_im_notional: BalanceResponseSummaryPortfolioImNotional,
    pub portfolio_maintenance_margin: String,
    pub portfolio_mm_notional: BalanceResponseSummaryPortfolioMmNotional,
    pub liquidation_percentage: String,
    pub liquidation_buffer: String,
    pub margin_type: String,
    pub margin_flags: String,
    pub liquidation_status: String,
    pub unrealized_pnl: BalanceResponseSummaryUnrealizedPnl,
    pub buying_power: BalanceResponseSummaryBuyingPower,
    pub total_balance: BalanceResponseSummaryTotalBalance,
    pub max_withdrawal: BalanceResponseSummaryMaxWithdrawal,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseSummary {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            portfolio_uuid: String,
            collateral: String,
            position_notional: String,
            open_position_notional: String,
            pending_fees: String,
            borrow: String,
            accrued_interest: String,
            rolling_debt: String,
            portfolio_initial_margin: String,
            portfolio_im_notional: BalanceResponseSummaryPortfolioImNotional,
            portfolio_maintenance_margin: String,
            portfolio_mm_notional: BalanceResponseSummaryPortfolioMmNotional,
            liquidation_percentage: String,
            liquidation_buffer: String,
            margin_type: String,
            margin_flags: String,
            liquidation_status: String,
            unrealized_pnl: BalanceResponseSummaryUnrealizedPnl,
            buying_power: BalanceResponseSummaryBuyingPower,
            total_balance: BalanceResponseSummaryTotalBalance,
            max_withdrawal: BalanceResponseSummaryMaxWithdrawal,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            portfolio_uuid: value.portfolio_uuid,
            collateral: value.collateral,
            position_notional: value.position_notional,
            open_position_notional: value.open_position_notional,
            pending_fees: value.pending_fees,
            borrow: value.borrow,
            accrued_interest: value.accrued_interest,
            rolling_debt: value.rolling_debt,
            portfolio_initial_margin: value.portfolio_initial_margin,
            portfolio_im_notional: value.portfolio_im_notional,
            portfolio_maintenance_margin: value.portfolio_maintenance_margin,
            portfolio_mm_notional: value.portfolio_mm_notional,
            liquidation_percentage: value.liquidation_percentage,
            liquidation_buffer: value.liquidation_buffer,
            margin_type: value.margin_type,
            margin_flags: value.margin_flags,
            liquidation_status: value.liquidation_status,
            unrealized_pnl: value.unrealized_pnl,
            buying_power: value.buying_power,
            total_balance: value.total_balance,
            max_withdrawal: value.max_withdrawal,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponse {
    pub summary: BalanceResponseSummary,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            summary: BalanceResponseSummary,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            summary: value.summary,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderEventType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "update")]
    Update,
}
impl<'de> Deserialize<'de> for OrderEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "snapshot")]
            Snapshot,
            #[serde(rename = "update")]
            Update,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Snapshot => Ok(Self::Snapshot),
            Wire::Update => Ok(Self::Update),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderEventOrdersItemStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "FAILED")]
    Failed,
}
impl<'de> Deserialize<'de> for OrderEventOrdersItemStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "PENDING")]
            Pending,
            #[serde(rename = "OPEN")]
            Open,
            #[serde(rename = "FILLED")]
            Filled,
            #[serde(rename = "CANCELLED")]
            Cancelled,
            #[serde(rename = "EXPIRED")]
            Expired,
            #[serde(rename = "FAILED")]
            Failed,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Pending => Ok(Self::Pending),
            Wire::Open => Ok(Self::Open),
            Wire::Filled => Ok(Self::Filled),
            Wire::Cancelled => Ok(Self::Cancelled),
            Wire::Expired => Ok(Self::Expired),
            Wire::Failed => Ok(Self::Failed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderEventOrdersItemOrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for OrderEventOrdersItemOrderSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "BUY")]
            Buy,
            #[serde(rename = "SELL")]
            Sell,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Buy => Ok(Self::Buy),
            Wire::Sell => Ok(Self::Sell),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderEventOrdersItemOrderType {
    #[serde(rename = "Limit")]
    Limit,
    #[serde(rename = "Market")]
    Market,
    #[serde(rename = "Stop Limit")]
    StopLimit,
}
impl<'de> Deserialize<'de> for OrderEventOrdersItemOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "Limit")]
            Limit,
            #[serde(rename = "Market")]
            Market,
            #[serde(rename = "Stop Limit")]
            StopLimit,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Limit => Ok(Self::Limit),
            Wire::Market => Ok(Self::Market),
            Wire::StopLimit => Ok(Self::StopLimit),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderEventOrdersItem {
    pub order_id: String,
    pub client_order_id: String,
    pub cumulative_quantity: String,
    pub leaves_quantity: String,
    pub avg_price: String,
    pub limit_price: String,
    pub stop_price: String,
    pub total_fees: String,
    pub status: OrderEventOrdersItemStatus,
    pub product_id: String,
    pub creation_time: String,
    pub order_side: OrderEventOrdersItemOrderSide,
    pub order_type: OrderEventOrdersItemOrderType,
    pub cancel_reason: String,
    pub reject_reason: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderEventOrdersItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            order_id: String,
            client_order_id: String,
            cumulative_quantity: String,
            leaves_quantity: String,
            avg_price: String,
            limit_price: String,
            stop_price: String,
            total_fees: String,
            status: OrderEventOrdersItemStatus,
            product_id: String,
            creation_time: String,
            order_side: OrderEventOrdersItemOrderSide,
            order_type: OrderEventOrdersItemOrderType,
            cancel_reason: String,
            reject_reason: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            order_id: value.order_id,
            client_order_id: value.client_order_id,
            cumulative_quantity: value.cumulative_quantity,
            leaves_quantity: value.leaves_quantity,
            avg_price: value.avg_price,
            limit_price: value.limit_price,
            stop_price: value.stop_price,
            total_fees: value.total_fees,
            status: value.status,
            product_id: value.product_id,
            creation_time: value.creation_time,
            order_side: value.order_side,
            order_type: value.order_type,
            cancel_reason: value.cancel_reason,
            reject_reason: value.reject_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderEvent {
    pub r#type: OrderEventType,
    pub orders: Vec<OrderEventOrdersItem>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderEvent {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            r#type: OrderEventType,
            orders: Vec<OrderEventOrdersItem>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            r#type: value.r#type,
            orders: value.orders,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListOrdersResponseOrdersItemSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for ListOrdersResponseOrdersItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "BUY")]
            Buy,
            #[serde(rename = "SELL")]
            Sell,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Buy => Ok(Self::Buy),
            Wire::Sell => Ok(Self::Sell),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListOrdersResponseOrdersItemStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "UNKNOWN_ORDER_STATUS")]
    UnknownOrderStatus,
    #[serde(rename = "QUEUED")]
    Queued,
    #[serde(rename = "CANCEL_QUEUED")]
    CancelQueued,
    #[serde(rename = "EDIT_QUEUED")]
    EditQueued,
}
impl<'de> Deserialize<'de> for ListOrdersResponseOrdersItemStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "PENDING")]
            Pending,
            #[serde(rename = "OPEN")]
            Open,
            #[serde(rename = "FILLED")]
            Filled,
            #[serde(rename = "CANCELLED")]
            Cancelled,
            #[serde(rename = "EXPIRED")]
            Expired,
            #[serde(rename = "FAILED")]
            Failed,
            #[serde(rename = "UNKNOWN_ORDER_STATUS")]
            UnknownOrderStatus,
            #[serde(rename = "QUEUED")]
            Queued,
            #[serde(rename = "CANCEL_QUEUED")]
            CancelQueued,
            #[serde(rename = "EDIT_QUEUED")]
            EditQueued,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Pending => Ok(Self::Pending),
            Wire::Open => Ok(Self::Open),
            Wire::Filled => Ok(Self::Filled),
            Wire::Cancelled => Ok(Self::Cancelled),
            Wire::Expired => Ok(Self::Expired),
            Wire::Failed => Ok(Self::Failed),
            Wire::UnknownOrderStatus => Ok(Self::UnknownOrderStatus),
            Wire::Queued => Ok(Self::Queued),
            Wire::CancelQueued => Ok(Self::CancelQueued),
            Wire::EditQueued => Ok(Self::EditQueued),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListOrdersResponseOrdersItemOrderType {
    #[serde(rename = "UNKNOWN_ORDER_TYPE")]
    UnknownOrderType,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "STOP_LIMIT")]
    StopLimit,
    #[serde(rename = "BRACKET")]
    Bracket,
    #[serde(rename = "TWAP")]
    Twap,
    #[serde(rename = "ROLL_OPEN")]
    RollOpen,
    #[serde(rename = "ROLL_CLOSE")]
    RollClose,
    #[serde(rename = "LIQUIDATION")]
    Liquidation,
    #[serde(rename = "SCALED")]
    Scaled,
}
impl<'de> Deserialize<'de> for ListOrdersResponseOrdersItemOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "UNKNOWN_ORDER_TYPE")]
            UnknownOrderType,
            #[serde(rename = "MARKET")]
            Market,
            #[serde(rename = "LIMIT")]
            Limit,
            #[serde(rename = "STOP")]
            Stop,
            #[serde(rename = "STOP_LIMIT")]
            StopLimit,
            #[serde(rename = "BRACKET")]
            Bracket,
            #[serde(rename = "TWAP")]
            Twap,
            #[serde(rename = "ROLL_OPEN")]
            RollOpen,
            #[serde(rename = "ROLL_CLOSE")]
            RollClose,
            #[serde(rename = "LIQUIDATION")]
            Liquidation,
            #[serde(rename = "SCALED")]
            Scaled,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::UnknownOrderType => Ok(Self::UnknownOrderType),
            Wire::Market => Ok(Self::Market),
            Wire::Limit => Ok(Self::Limit),
            Wire::Stop => Ok(Self::Stop),
            Wire::StopLimit => Ok(Self::StopLimit),
            Wire::Bracket => Ok(Self::Bracket),
            Wire::Twap => Ok(Self::Twap),
            Wire::RollOpen => Ok(Self::RollOpen),
            Wire::RollClose => Ok(Self::RollClose),
            Wire::Liquidation => Ok(Self::Liquidation),
            Wire::Scaled => Ok(Self::Scaled),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
            limit_price: value.limit_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub post_only: Option<bool>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            post_only: Option<bool>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
            limit_price: value.limit_price,
            post_only: value.post_only,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
            limit_price: value.limit_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            quote_size: value.quote_size,
            limit_price: value.limit_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_price: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_price: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_price: value.stop_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub base_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_price: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_price: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            base_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_price: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_price: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_price: value.stop_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItemOrderConfiguration {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub market_market_ioc: Option<ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub market_market_fok: Option<ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub sor_limit_ioc: Option<ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_limit_gtc: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_limit_gtd: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_limit_fok: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_limit_stop_limit_gtc:
        Option<ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_limit_stop_limit_gtd:
        Option<ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for ListOrdersResponseOrdersItemOrderConfiguration
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            market_market_ioc:
                Option<ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            market_market_fok:
                Option<ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            sor_limit_ioc: Option<ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_limit_gtc: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_limit_gtd: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_limit_fok: Option<ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_limit_stop_limit_gtc:
                Option<ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_limit_stop_limit_gtd:
                Option<ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            market_market_ioc: value.market_market_ioc,
            market_market_fok: value.market_market_fok,
            sor_limit_ioc: value.sor_limit_ioc,
            limit_limit_gtc: value.limit_limit_gtc,
            limit_limit_gtd: value.limit_limit_gtd,
            limit_limit_fok: value.limit_limit_fok,
            stop_limit_stop_limit_gtc: value.stop_limit_stop_limit_gtc,
            stop_limit_stop_limit_gtd: value.stop_limit_stop_limit_gtd,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponseOrdersItem {
    pub order_id: String,
    pub client_order_id: String,
    pub product_id: String,
    pub side: ListOrdersResponseOrdersItemSide,
    pub status: ListOrdersResponseOrdersItemStatus,
    pub order_type: ListOrdersResponseOrdersItemOrderType,
    pub created_time: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub filled_size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub average_filled_price: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub total_fees: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub order_configuration: Option<ListOrdersResponseOrdersItemOrderConfiguration>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListOrdersResponseOrdersItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            order_id: String,
            client_order_id: String,
            product_id: String,
            side: ListOrdersResponseOrdersItemSide,
            status: ListOrdersResponseOrdersItemStatus,
            order_type: ListOrdersResponseOrdersItemOrderType,
            created_time: String,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            filled_size: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            average_filled_price: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            total_fees: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            order_configuration: Option<ListOrdersResponseOrdersItemOrderConfiguration>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            order_id: value.order_id,
            client_order_id: value.client_order_id,
            product_id: value.product_id,
            side: value.side,
            status: value.status,
            order_type: value.order_type,
            created_time: value.created_time,
            filled_size: value.filled_size,
            average_filled_price: value.average_filled_price,
            total_fees: value.total_fees,
            order_configuration: value.order_configuration,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct ListOrdersResponse {
    pub orders: Vec<ListOrdersResponseOrdersItem>,
    pub has_next: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub cursor: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ListOrdersResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            orders: Vec<ListOrdersResponseOrdersItem>,
            has_next: bool,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            cursor: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            orders: value.orders,
            has_next: value.has_next,
            cursor: value.cursor,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StopDirection {
    #[serde(rename = "STOP_DIRECTION_STOP_UP")]
    StopDirectionStopUp,
    #[serde(rename = "STOP_DIRECTION_STOP_DOWN")]
    StopDirectionStopDown,
}
impl<'de> Deserialize<'de> for StopDirection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "STOP_DIRECTION_STOP_UP")]
            StopDirectionStopUp,
            #[serde(rename = "STOP_DIRECTION_STOP_DOWN")]
            StopDirectionStopDown,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::StopDirectionStopUp => Ok(Self::StopDirectionStopUp),
            Wire::StopDirectionStopDown => Ok(Self::StopDirectionStopDown),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MarginType {
    #[serde(rename = "ISOLATED")]
    Isolated,
    #[serde(rename = "CROSS")]
    Cross,
}
impl<'de> Deserialize<'de> for MarginType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "ISOLATED")]
            Isolated,
            #[serde(rename = "CROSS")]
            Cross,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Isolated => Ok(Self::Isolated),
            Wire::Cross => Ok(Self::Cross),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigMarketMarketIoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub quote_size: Option<String>,
    pub base_size: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigMarketMarketIoc {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            quote_size: Option<String>,
            base_size: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            quote_size: value.quote_size,
            base_size: value.base_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigSorLimitIoc {
    pub base_size: String,
    pub limit_price: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigSorLimitIoc {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigLimitLimitGtc {
    pub base_size: String,
    pub limit_price: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub post_only: Option<bool>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigLimitLimitGtc {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            post_only: Option<bool>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            post_only: value.post_only,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigLimitLimitGtd {
    pub base_size: String,
    pub limit_price: String,
    pub end_time: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub post_only: Option<bool>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigLimitLimitGtd {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            end_time: String,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            post_only: Option<bool>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            end_time: value.end_time,
            post_only: value.post_only,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigStopLimitStopLimitGtc {
    pub base_size: String,
    pub limit_price: String,
    pub stop_price: String,
    pub stop_direction: StopDirection,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for OrderRequestConfigStopLimitStopLimitGtc
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            stop_price: String,
            stop_direction: StopDirection,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_price: value.stop_price,
            stop_direction: value.stop_direction,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigStopLimitStopLimitGtd {
    pub base_size: String,
    pub limit_price: String,
    pub stop_price: String,
    pub end_time: String,
    pub stop_direction: StopDirection,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for OrderRequestConfigStopLimitStopLimitGtd
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            stop_price: String,
            end_time: String,
            stop_direction: StopDirection,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_price: value.stop_price,
            end_time: value.end_time,
            stop_direction: value.stop_direction,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigTriggerBracketGtc {
    pub base_size: String,
    pub limit_price: String,
    pub stop_trigger_price: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigTriggerBracketGtc {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            stop_trigger_price: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_trigger_price: value.stop_trigger_price,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfigTriggerBracketGtd {
    pub base_size: String,
    pub limit_price: String,
    pub stop_trigger_price: String,
    pub end_time: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfigTriggerBracketGtd {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            base_size: String,
            limit_price: String,
            stop_trigger_price: String,
            end_time: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            base_size: value.base_size,
            limit_price: value.limit_price,
            stop_trigger_price: value.stop_trigger_price,
            end_time: value.end_time,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct OrderRequestConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub market_market_ioc: Option<OrderRequestConfigMarketMarketIoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub sor_limit_ioc: Option<OrderRequestConfigSorLimitIoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_limit_gtc: Option<OrderRequestConfigLimitLimitGtc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub limit_limit_gtd: Option<OrderRequestConfigLimitLimitGtd>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_limit_stop_limit_gtc: Option<OrderRequestConfigStopLimitStopLimitGtc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stop_limit_stop_limit_gtd: Option<OrderRequestConfigStopLimitStopLimitGtd>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub trigger_bracket_gtc: Option<OrderRequestConfigTriggerBracketGtc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub trigger_bracket_gtd: Option<OrderRequestConfigTriggerBracketGtd>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for OrderRequestConfig {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            market_market_ioc: Option<OrderRequestConfigMarketMarketIoc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            sor_limit_ioc: Option<OrderRequestConfigSorLimitIoc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_limit_gtc: Option<OrderRequestConfigLimitLimitGtc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            limit_limit_gtd: Option<OrderRequestConfigLimitLimitGtd>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_limit_stop_limit_gtc: Option<OrderRequestConfigStopLimitStopLimitGtc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            stop_limit_stop_limit_gtd: Option<OrderRequestConfigStopLimitStopLimitGtd>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            trigger_bracket_gtc: Option<OrderRequestConfigTriggerBracketGtc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            trigger_bracket_gtd: Option<OrderRequestConfigTriggerBracketGtd>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            market_market_ioc: value.market_market_ioc,
            sor_limit_ioc: value.sor_limit_ioc,
            limit_limit_gtc: value.limit_limit_gtc,
            limit_limit_gtd: value.limit_limit_gtd,
            stop_limit_stop_limit_gtc: value.stop_limit_stop_limit_gtc,
            stop_limit_stop_limit_gtd: value.stop_limit_stop_limit_gtd,
            trigger_bracket_gtc: value.trigger_bracket_gtc,
            trigger_bracket_gtd: value.trigger_bracket_gtd,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PlaceOrderRequestSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for PlaceOrderRequestSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "BUY")]
            Buy,
            #[serde(rename = "SELL")]
            Sell,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Buy => Ok(Self::Buy),
            Wire::Sell => Ok(Self::Sell),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct PlaceOrderRequest {
    pub client_order_id: String,
    pub product_id: String,
    pub side: PlaceOrderRequestSide,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub leverage: Option<String>,
    pub order_configuration: OrderRequestConfig,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub self_trade_prevention_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub margin_type: Option<MarginType>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub retail_portfolio_id: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for PlaceOrderRequest {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            client_order_id: String,
            product_id: String,
            side: PlaceOrderRequestSide,
            order_configuration: OrderRequestConfig,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            self_trade_prevention_id: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            leverage: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            margin_type: Option<MarginType>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            retail_portfolio_id: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            client_order_id: value.client_order_id,
            product_id: value.product_id,
            side: value.side,
            order_configuration: value.order_configuration,
            self_trade_prevention_id: value.self_trade_prevention_id,
            leverage: value.leverage,
            margin_type: value.margin_type,
            retail_portfolio_id: value.retail_portfolio_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct PlaceOrderResponseSuccessResponse {
    pub order_id: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for PlaceOrderResponseSuccessResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            order_id: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            order_id: value.order_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct PlaceOrderResponseErrorResponse {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub error: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub message: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub error_details: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub preview_failure_reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub new_order_failure_reason: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for PlaceOrderResponseErrorResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            error: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            message: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            error_details: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            preview_failure_reason: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            new_order_failure_reason: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            error: value.error,
            message: value.message,
            error_details: value.error_details,
            preview_failure_reason: value.preview_failure_reason,
            new_order_failure_reason: value.new_order_failure_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct PlaceOrderResponse {
    pub success: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub success_response: Option<PlaceOrderResponseSuccessResponse>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub failure_reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub order_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub error_response: Option<PlaceOrderResponseErrorResponse>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub order_configuration: Option<OrderRequestConfig>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for PlaceOrderResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            success: bool,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            success_response: Option<PlaceOrderResponseSuccessResponse>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            failure_reason: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            order_id: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            error_response: Option<PlaceOrderResponseErrorResponse>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            order_configuration: Option<OrderRequestConfig>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            success: value.success,
            success_response: value.success_response,
            failure_reason: value.failure_reason,
            order_id: value.order_id,
            error_response: value.error_response,
            order_configuration: value.order_configuration,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct EditOrderRequest {
    pub order_id: String,
    pub price: String,
    pub size: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for EditOrderRequest {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            order_id: String,
            price: String,
            size: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            order_id: value.order_id,
            price: value.price,
            size: value.size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct EditOrderResponseErrorsItem {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub edit_failure_reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub preview_failure_reason: Option<String>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for EditOrderResponseErrorsItem {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            edit_failure_reason: Option<String>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            preview_failure_reason: Option<String>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            edit_failure_reason: value.edit_failure_reason,
            preview_failure_reason: value.preview_failure_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct EditOrderResponse {
    pub success: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub errors: Option<Vec<EditOrderResponseErrorsItem>>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for EditOrderResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            success: bool,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            errors: Option<Vec<EditOrderResponseErrorsItem>>,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            success: value.success,
            errors: value.errors,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryFuturesBuyingPower {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryFuturesBuyingPower
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryTotalUsdBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryTotalUsdBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryCbiUsdBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryCbiUsdBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryCfmUsdBalance {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryCfmUsdBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryUnrealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryDailyRealizedPnl {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryDailyRealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryInitialMargin {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryInitialMargin
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryAvailableMargin {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryAvailableMargin
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryLiquidationThreshold {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryLiquidationThreshold
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryLiquidationBufferAmount {
    pub value: String,
    pub currency: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryLiquidationBufferAmount
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            value: String,
            currency: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            value: value.value,
            currency: value.currency,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure {
    pub margin_window_type: String,
    pub margin_level: String,
    pub initial_margin: String,
    pub maintenance_margin: String,
    pub liquidation_buffer: String,
    pub total_hold: String,
    pub futures_buying_power: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            margin_window_type: String,
            margin_level: String,
            initial_margin: String,
            maintenance_margin: String,
            liquidation_buffer: String,
            total_hold: String,
            futures_buying_power: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            margin_window_type: value.margin_window_type,
            margin_level: value.margin_level,
            initial_margin: value.initial_margin,
            maintenance_margin: value.maintenance_margin,
            liquidation_buffer: value.liquidation_buffer,
            total_hold: value.total_hold,
            futures_buying_power: value.futures_buying_power,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure {
    pub margin_window_type: String,
    pub margin_level: String,
    pub initial_margin: String,
    pub maintenance_margin: String,
    pub liquidation_buffer: String,
    pub total_hold: String,
    pub futures_buying_power: String,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            margin_window_type: String,
            margin_level: String,
            initial_margin: String,
            maintenance_margin: String,
            liquidation_buffer: String,
            total_hold: String,
            futures_buying_power: String,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            margin_window_type: value.margin_window_type,
            margin_level: value.margin_level,
            initial_margin: value.initial_margin,
            maintenance_margin: value.maintenance_margin,
            liquidation_buffer: value.liquidation_buffer,
            total_hold: value.total_hold,
            futures_buying_power: value.futures_buying_power,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfmBalanceSummary {
    pub futures_buying_power: BalanceResponseCfmBalanceSummaryFuturesBuyingPower,
    pub total_usd_balance: BalanceResponseCfmBalanceSummaryTotalUsdBalance,
    pub cbi_usd_balance: BalanceResponseCfmBalanceSummaryCbiUsdBalance,
    pub cfm_usd_balance: BalanceResponseCfmBalanceSummaryCfmUsdBalance,
    pub total_open_orders_hold_amount: BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount,
    pub unrealized_pnl: BalanceResponseCfmBalanceSummaryUnrealizedPnl,
    pub daily_realized_pnl: BalanceResponseCfmBalanceSummaryDailyRealizedPnl,
    pub initial_margin: BalanceResponseCfmBalanceSummaryInitialMargin,
    pub available_margin: BalanceResponseCfmBalanceSummaryAvailableMargin,
    pub liquidation_threshold: BalanceResponseCfmBalanceSummaryLiquidationThreshold,
    pub liquidation_buffer_amount: BalanceResponseCfmBalanceSummaryLiquidationBufferAmount,
    pub liquidation_buffer_percentage: String,
    pub intraday_margin_window_measure: BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure,
    pub overnight_margin_window_measure:
        BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseCfmBalanceSummary {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            futures_buying_power: BalanceResponseCfmBalanceSummaryFuturesBuyingPower,
            total_usd_balance: BalanceResponseCfmBalanceSummaryTotalUsdBalance,
            cbi_usd_balance: BalanceResponseCfmBalanceSummaryCbiUsdBalance,
            cfm_usd_balance: BalanceResponseCfmBalanceSummaryCfmUsdBalance,
            total_open_orders_hold_amount:
                BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount,
            unrealized_pnl: BalanceResponseCfmBalanceSummaryUnrealizedPnl,
            daily_realized_pnl: BalanceResponseCfmBalanceSummaryDailyRealizedPnl,
            initial_margin: BalanceResponseCfmBalanceSummaryInitialMargin,
            available_margin: BalanceResponseCfmBalanceSummaryAvailableMargin,
            liquidation_threshold: BalanceResponseCfmBalanceSummaryLiquidationThreshold,
            liquidation_buffer_amount: BalanceResponseCfmBalanceSummaryLiquidationBufferAmount,
            liquidation_buffer_percentage: String,
            intraday_margin_window_measure:
                BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure,
            overnight_margin_window_measure:
                BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            futures_buying_power: value.futures_buying_power,
            total_usd_balance: value.total_usd_balance,
            cbi_usd_balance: value.cbi_usd_balance,
            cfm_usd_balance: value.cfm_usd_balance,
            total_open_orders_hold_amount: value.total_open_orders_hold_amount,
            unrealized_pnl: value.unrealized_pnl,
            daily_realized_pnl: value.daily_realized_pnl,
            initial_margin: value.initial_margin,
            available_margin: value.available_margin,
            liquidation_threshold: value.liquidation_threshold,
            liquidation_buffer_amount: value.liquidation_buffer_amount,
            liquidation_buffer_percentage: value.liquidation_buffer_percentage,
            intraday_margin_window_measure: value.intraday_margin_window_measure,
            overnight_margin_window_measure: value.overnight_margin_window_measure,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct BalanceResponseCfm {
    pub balance_summary: BalanceResponseCfmBalanceSummary,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for BalanceResponseCfm {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            balance_summary: BalanceResponseCfmBalanceSummary,
        }
        let value: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            balance_summary: value.balance_summary,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolio {
    pub name: String,
    pub uuid: String,
    pub r#type: String,
    pub deleted: bool,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolio")]
struct PortfolioBreakdownResponseBreakdownPortfolioWire {
    name: String,
    uuid: String,
    r#type: String,
    deleted: bool,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownPortfolio {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolio
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, uuid, r#type, deleted });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolio
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalanceWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalanceWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalanceWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalanceWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalanceWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnlWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnlWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnlWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnlWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPortfolioBalances {
    pub total_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance,
    pub total_futures_balance:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance,
    pub total_cash_equivalent_balance:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance,
    pub total_crypto_balance:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance,
    pub futures_unrealized_pnl:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl,
    pub perp_unrealized_pnl: PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPortfolioBalances")]
struct PortfolioBreakdownResponseBreakdownPortfolioBalancesWire {
    total_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance,
    total_futures_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance,
    total_cash_equivalent_balance:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance,
    total_crypto_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance,
    futures_unrealized_pnl:
        PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl,
    perp_unrealized_pnl: PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownPortfolioBalances {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPortfolioBalancesWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPortfolioBalances
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { total_balance, total_futures_balance, total_cash_equivalent_balance, total_crypto_balance, futures_unrealized_pnl, perp_unrealized_pnl });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPortfolioBalances
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis")]
struct PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasisWire {
    value: String,
    currency: String,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasisWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, currency });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownSpotPositionsItem {
    pub asset: String,
    pub account_uuid: String,
    pub total_balance_fiat: f64,
    pub total_balance_crypto: f64,
    pub available_to_trade_fiat: f64,
    pub allocation: f64,
    pub cost_basis: PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis,
    pub asset_img_url: String,
    pub is_cash: bool,
    pub average_entry_price: String,
    pub asset_uuid: String,
    pub available_to_trade_crypto: f64,
    pub unrealized_pnl: f64,
    pub available_to_transfer_fiat: f64,
    pub available_to_transfer_crypto: f64,
    pub asset_color: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownSpotPositionsItem")]
struct PortfolioBreakdownResponseBreakdownSpotPositionsItemWire {
    asset: String,
    account_uuid: String,
    total_balance_fiat: f64,
    total_balance_crypto: f64,
    available_to_trade_fiat: f64,
    allocation: f64,
    cost_basis: PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis,
    asset_img_url: String,
    is_cash: bool,
    average_entry_price: String,
    asset_uuid: String,
    available_to_trade_crypto: f64,
    unrealized_pnl: f64,
    available_to_transfer_fiat: f64,
    available_to_transfer_crypto: f64,
    asset_color: String,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownSpotPositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownSpotPositionsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownSpotPositionsItem
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { asset, account_uuid, total_balance_fiat, total_balance_crypto, available_to_trade_fiat, allocation, cost_basis, asset_img_url, is_cash, average_entry_price, asset_uuid, available_to_trade_crypto, unrealized_pnl, available_to_transfer_fiat, available_to_transfer_crypto, asset_color });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownSpotPositionsItem
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PortfolioBreakdownResponseBreakdownPerpPositionsItemSide {
    #[serde(rename = "FUTURES_POSITION_SIDE_LONG")]
    FuturesPositionSideLong,
    #[serde(rename = "FUTURES_POSITION_SIDE_SHORT")]
    FuturesPositionSideShort,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownPerpPositionsItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "FUTURES_POSITION_SIDE_LONG")]
            FuturesPositionSideLong,
            #[serde(rename = "FUTURES_POSITION_SIDE_SHORT")]
            FuturesPositionSideShort,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::FuturesPositionSideLong => Ok(Self::FuturesPositionSideLong),
            Wire::FuturesPositionSideShort => Ok(Self::FuturesPositionSideShort),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownPerpPositionsItem {
    pub product_id: String,
    pub contract_size: String,
    pub side: PortfolioBreakdownResponseBreakdownPerpPositionsItemSide,
    pub amount: String,
    pub avg_entry_price: String,
    pub current_price: String,
    pub unrealized_pnl: String,
    pub expiry: String,
    pub underlying_asset: String,
    pub asset_img_url: String,
    pub product_name: String,
    pub venue: String,
    pub notional_value: String,
    pub asset_color: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownPerpPositionsItem")]
struct PortfolioBreakdownResponseBreakdownPerpPositionsItemWire {
    product_id: String,
    contract_size: String,
    side: PortfolioBreakdownResponseBreakdownPerpPositionsItemSide,
    amount: String,
    avg_entry_price: String,
    current_price: String,
    unrealized_pnl: String,
    expiry: String,
    underlying_asset: String,
    asset_img_url: String,
    product_name: String,
    venue: String,
    notional_value: String,
    asset_color: String,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownPerpPositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownPerpPositionsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPerpPositionsItem
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { product_id, contract_size, side, amount, avg_entry_price, current_price, unrealized_pnl, expiry, underlying_asset, asset_img_url, product_name, venue, notional_value, asset_color });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownPerpPositionsItem
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide {
    #[serde(rename = "FUTURES_POSITION_SIDE_LONG")]
    FuturesPositionSideLong,
    #[serde(rename = "FUTURES_POSITION_SIDE_SHORT")]
    FuturesPositionSideShort,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "FUTURES_POSITION_SIDE_LONG")]
            FuturesPositionSideLong,
            #[serde(rename = "FUTURES_POSITION_SIDE_SHORT")]
            FuturesPositionSideShort,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::FuturesPositionSideLong => Ok(Self::FuturesPositionSideLong),
            Wire::FuturesPositionSideShort => Ok(Self::FuturesPositionSideShort),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdownFuturesPositionsItem {
    pub product_id: String,
    pub contract_size: String,
    pub side: PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide,
    pub amount: String,
    pub avg_entry_price: String,
    pub current_price: String,
    pub unrealized_pnl: String,
    pub expiry: String,
    pub underlying_asset: String,
    pub asset_img_url: String,
    pub product_name: String,
    pub venue: String,
    pub notional_value: String,
    pub asset_color: String,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdownFuturesPositionsItem")]
struct PortfolioBreakdownResponseBreakdownFuturesPositionsItemWire {
    product_id: String,
    contract_size: String,
    side: PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide,
    amount: String,
    avg_entry_price: String,
    current_price: String,
    unrealized_pnl: String,
    expiry: String,
    underlying_asset: String,
    asset_img_url: String,
    product_name: String,
    venue: String,
    notional_value: String,
    asset_color: String,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdownFuturesPositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownFuturesPositionsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownFuturesPositionsItem
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { product_id, contract_size, side, amount, avg_entry_price, current_price, unrealized_pnl, expiry, underlying_asset, asset_img_url, product_name, venue, notional_value, asset_color });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for PortfolioBreakdownResponseBreakdownFuturesPositionsItem
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponseBreakdown {
    pub portfolio: PortfolioBreakdownResponseBreakdownPortfolio,
    pub portfolio_balances: PortfolioBreakdownResponseBreakdownPortfolioBalances,
    pub spot_positions: Vec<PortfolioBreakdownResponseBreakdownSpotPositionsItem>,
    pub perp_positions: Vec<PortfolioBreakdownResponseBreakdownPerpPositionsItem>,
    pub futures_positions: Vec<PortfolioBreakdownResponseBreakdownFuturesPositionsItem>,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponseBreakdown")]
struct PortfolioBreakdownResponseBreakdownWire {
    portfolio: PortfolioBreakdownResponseBreakdownPortfolio,
    portfolio_balances: PortfolioBreakdownResponseBreakdownPortfolioBalances,
    spot_positions: Vec<PortfolioBreakdownResponseBreakdownSpotPositionsItem>,
    perp_positions: Vec<PortfolioBreakdownResponseBreakdownPerpPositionsItem>,
    futures_positions: Vec<PortfolioBreakdownResponseBreakdownFuturesPositionsItem>,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponseBreakdown {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseBreakdownWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PortfolioBreakdownResponseBreakdown {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { portfolio, portfolio_balances, spot_positions, perp_positions, futures_positions });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for PortfolioBreakdownResponseBreakdown {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortfolioBreakdownResponse {
    pub breakdown: PortfolioBreakdownResponseBreakdown,
}

#[derive(Deserialize)]
#[serde(remote = "PortfolioBreakdownResponse")]
struct PortfolioBreakdownResponseWire {
    breakdown: PortfolioBreakdownResponseBreakdown,
}
impl<'de> Deserialize<'de> for PortfolioBreakdownResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PortfolioBreakdownResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PortfolioBreakdownResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { breakdown });
    }
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for PortfolioBreakdownResponse {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        Self::deserialize(serde_json::Value::Object(map))
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownPerpPositionsItemSide
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { FuturesPositionSideLong, FuturesPositionSideShort });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { FuturesPositionSideLong, FuturesPositionSideShort });
    }
}
