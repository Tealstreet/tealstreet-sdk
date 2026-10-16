//! Wire declarations corresponding to backpack.types.ts; source field spelling is preserved.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(decoder)
}
// ts: BackpackAsset
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackAsset {
    pub blockchain: String,
    pub depositEnabled: bool,
    pub minimumDeposit: String,
    pub withdrawEnabled: bool,
    pub minimumWithdrawal: String,
    pub maximumWithdrawal: String,
    pub withdrawalFee: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackAsset")]
struct BackpackAssetWire {
    blockchain: String,
    depositEnabled: bool,
    minimumDeposit: String,
    withdrawEnabled: bool,
    minimumWithdrawal: String,
    maximumWithdrawal: String,
    withdrawalFee: String,
}
impl Serialize for BackpackAsset {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackAssetWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackAsset {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackAssetWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackAsset {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { blockchain, depositEnabled, minimumDeposit, withdrawEnabled, minimumWithdrawal, maximumWithdrawal, withdrawalFee });
    }
}

// ts: BackpackMarket (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackMarketMarketType {
    Spot,
    Perp,
}
impl Serialize for BackpackMarketMarketType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Spot => "SPOT",
            Self::Perp => "PERP",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackMarketMarketType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "SPOT" => Ok(Self::Spot),
            "PERP" => Ok(Self::Perp),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackMarketMarketType literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackMarketMarketType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Spot, Perp });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketImfFunction {
    pub base: String,
    pub factor: String,
    pub r#type: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketImfFunction")]
struct BackpackMarketImfFunctionWire {
    base: String,
    factor: String,
    r#type: String,
}
impl Serialize for BackpackMarketImfFunction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketImfFunctionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketImfFunction {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketImfFunctionWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketImfFunction {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { base, factor, r#type });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketMmfFunction {
    pub base: String,
    pub factor: String,
    pub r#type: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketMmfFunction")]
struct BackpackMarketMmfFunctionWire {
    base: String,
    factor: String,
    r#type: String,
}
impl Serialize for BackpackMarketMmfFunction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketMmfFunctionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketMmfFunction {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketMmfFunctionWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketMmfFunction {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { base, factor, r#type });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketFiltersPrice {
    pub minPrice: Option<String>,
    pub maxPrice: Option<String>,
    pub tickSize: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketFiltersPrice")]
struct BackpackMarketFiltersPriceWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    minPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    maxPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    tickSize: Option<String>,
}
impl Serialize for BackpackMarketFiltersPrice {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketFiltersPriceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketFiltersPrice {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketFiltersPriceWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketFiltersPrice {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { minPrice, maxPrice, tickSize });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketFiltersQuantity {
    pub minQuantity: Option<String>,
    pub maxQuantity: Option<String>,
    pub stepSize: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketFiltersQuantity")]
struct BackpackMarketFiltersQuantityWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    minQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    maxQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    stepSize: Option<String>,
}
impl Serialize for BackpackMarketFiltersQuantity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketFiltersQuantityWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketFiltersQuantity {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketFiltersQuantityWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketFiltersQuantity {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { minQuantity, maxQuantity, stepSize });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketFiltersLeverage {
    pub minLeverage: Option<f64>,
    pub maxLeverage: Option<f64>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketFiltersLeverage")]
struct BackpackMarketFiltersLeverageWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    minLeverage: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    maxLeverage: Option<f64>,
}
impl Serialize for BackpackMarketFiltersLeverage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketFiltersLeverageWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketFiltersLeverage {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketFiltersLeverageWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketFiltersLeverage {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { minLeverage, maxLeverage });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarketFilters {
    pub price: Option<BackpackMarketFiltersPrice>,
    pub quantity: Option<BackpackMarketFiltersQuantity>,
    pub leverage: Option<BackpackMarketFiltersLeverage>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarketFilters")]
struct BackpackMarketFiltersWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    price: Option<BackpackMarketFiltersPrice>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    quantity: Option<BackpackMarketFiltersQuantity>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    leverage: Option<BackpackMarketFiltersLeverage>,
}
impl Serialize for BackpackMarketFilters {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketFiltersWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarketFilters {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketFiltersWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarketFilters {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { price, quantity, leverage });
    }
}

// ts: BackpackMarket
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarket {
    pub symbol: String,
    pub baseSymbol: String,
    pub quoteSymbol: String,
    pub marketType: BackpackMarketMarketType,
    pub visible: bool,
    pub orderBookState: String,
    pub priceIncrement: String,
    pub quantityIncrement: String,
    pub quoteIncrement: String,
    pub minNotional: String,
    pub maxNotional: String,
    pub minQuantity: String,
    pub maxQuantity: String,
    pub takerFeeRate: Option<String>,
    pub makerFeeRate: Option<String>,
    pub imfFunction: Option<BackpackMarketImfFunction>,
    pub mmfFunction: Option<BackpackMarketMmfFunction>,
    pub filters: Option<BackpackMarketFilters>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarket")]
struct BackpackMarketWire {
    symbol: String,
    baseSymbol: String,
    quoteSymbol: String,
    marketType: BackpackMarketMarketType,
    visible: bool,
    orderBookState: String,
    priceIncrement: String,
    quantityIncrement: String,
    quoteIncrement: String,
    minNotional: String,
    maxNotional: String,
    minQuantity: String,
    maxQuantity: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    takerFeeRate: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    makerFeeRate: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    imfFunction: Option<BackpackMarketImfFunction>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    mmfFunction: Option<BackpackMarketMmfFunction>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    filters: Option<BackpackMarketFilters>,
}
impl Serialize for BackpackMarket {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarketWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarket {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarketWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarket {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, baseSymbol, quoteSymbol, marketType, visible, orderBookState, priceIncrement, quantityIncrement, quoteIncrement, minNotional, maxNotional, minQuantity, maxQuantity, takerFeeRate, makerFeeRate, imfFunction, mmfFunction, filters });
    }
}

// ts: BackpackTicker
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackTicker {
    pub symbol: String,
    pub firstPrice: String,
    pub lastPrice: String,
    pub priceChange: String,
    pub priceChangePercent: String,
    pub high: String,
    pub low: String,
    pub volume: String,
    pub quoteVolume: String,
    pub trades: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackTicker")]
struct BackpackTickerWire {
    symbol: String,
    firstPrice: String,
    lastPrice: String,
    priceChange: String,
    priceChangePercent: String,
    high: String,
    low: String,
    volume: String,
    quoteVolume: String,
    trades: f64,
}
impl Serialize for BackpackTicker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackTickerWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackTicker {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackTickerWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackTicker {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, firstPrice, lastPrice, priceChange, priceChangePercent, high, low, volume, quoteVolume, trades });
    }
}

// ts: BackpackDepth
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackDepth {
    pub lastUpdateId: f64,
    pub bids: Vec<(String, String)>,
    pub asks: Vec<(String, String)>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackDepth")]
struct BackpackDepthWire {
    lastUpdateId: f64,
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
}
impl Serialize for BackpackDepth {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackDepthWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackDepth {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackDepthWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackDepth {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { lastUpdateId, bids, asks });
    }
}

// ts: BackpackTrade
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackTrade {
    pub id: f64,
    pub price: String,
    pub quantity: String,
    pub quoteQuantity: String,
    pub timestamp: f64,
    pub isBuyerMaker: bool,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackTrade")]
struct BackpackTradeWire {
    id: f64,
    price: String,
    quantity: String,
    quoteQuantity: String,
    timestamp: f64,
    isBuyerMaker: bool,
}
impl Serialize for BackpackTrade {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackTradeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackTrade {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackTradeWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackTrade {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, price, quantity, quoteQuantity, timestamp, isBuyerMaker });
    }
}

// ts: BackpackKline
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackKline {
    pub start: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub quoteVolume: String,
    pub end: String,
    pub trades: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackKline")]
struct BackpackKlineWire {
    start: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    quoteVolume: String,
    end: String,
    trades: String,
}
impl Serialize for BackpackKline {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackKlineWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackKline {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackKlineWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackKline {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { start, open, high, low, close, volume, quoteVolume, end, trades });
    }
}

// ts: BackpackMarkPrice
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackMarkPrice {
    pub symbol: String,
    pub markPrice: String,
    pub indexPrice: String,
    pub fundingRate: String,
    pub nextFundingTimestamp: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackMarkPrice")]
struct BackpackMarkPriceWire {
    symbol: String,
    markPrice: String,
    indexPrice: String,
    fundingRate: String,
    nextFundingTimestamp: f64,
}
impl Serialize for BackpackMarkPrice {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackMarkPriceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackMarkPrice {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackMarkPriceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackMarkPrice {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, markPrice, indexPrice, fundingRate, nextFundingTimestamp });
    }
}

// ts: BackpackBalance
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackBalance {
    pub asset: Option<String>,
    pub available: String,
    pub locked: String,
    pub staked: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackBalance")]
struct BackpackBalanceWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    asset: Option<String>,
    available: String,
    locked: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    staked: Option<String>,
}
impl Serialize for BackpackBalance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackBalanceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackBalance {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackBalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackBalance {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { asset, available, locked, staked });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderSide {
    Bid,
    Ask,
}
impl Serialize for BackpackOrderSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderSide {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Bid" => Ok(Self::Bid),
            "Ask" => Ok(Self::Ask),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderSide literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Bid, Ask });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderOrderType {
    Limit,
    Market,
}
impl Serialize for BackpackOrderOrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Limit => "Limit",
            Self::Market => "Market",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Limit" => Ok(Self::Limit),
            "Market" => Ok(Self::Market),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderOrderType literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderTriggerBy {
    MarkPrice,
    IndexPrice,
    LastPrice,
}
impl Serialize for BackpackOrderTriggerBy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::MarkPrice => "MarkPrice",
            Self::IndexPrice => "IndexPrice",
            Self::LastPrice => "LastPrice",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderTriggerBy {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "MarkPrice" => Ok(Self::MarkPrice),
            "IndexPrice" => Ok(Self::IndexPrice),
            "LastPrice" => Ok(Self::LastPrice),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderTriggerBy literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderTriggerBy {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { MarkPrice, IndexPrice, LastPrice });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderTimeInForce {
    Gtc,
    Ioc,
    Fok,
    Po,
}
impl Serialize for BackpackOrderTimeInForce {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Gtc => "GTC",
            Self::Ioc => "IOC",
            Self::Fok => "FOK",
            Self::Po => "PO",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "GTC" => Ok(Self::Gtc),
            "IOC" => Ok(Self::Ioc),
            "FOK" => Ok(Self::Fok),
            "PO" => Ok(Self::Po),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderTimeInForce literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderTimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Gtc, Ioc, Fok, Po });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderSelfTradePrevention {
    RejectTaker,
    RejectMaker,
    RejectBoth,
    Allow,
}
impl Serialize for BackpackOrderSelfTradePrevention {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::RejectTaker => "RejectTaker",
            Self::RejectMaker => "RejectMaker",
            Self::RejectBoth => "RejectBoth",
            Self::Allow => "Allow",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderSelfTradePrevention {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "RejectTaker" => Ok(Self::RejectTaker),
            "RejectMaker" => Ok(Self::RejectMaker),
            "RejectBoth" => Ok(Self::RejectBoth),
            "Allow" => Ok(Self::Allow),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderSelfTradePrevention literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderSelfTradePrevention {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { RejectTaker, RejectMaker, RejectBoth, Allow });
    }
}

// ts: BackpackOrder (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Expired,
    TriggerPending,
}
impl Serialize for BackpackOrderStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::New => "New",
            Self::PartiallyFilled => "PartiallyFilled",
            Self::Filled => "Filled",
            Self::Cancelled => "Cancelled",
            Self::Expired => "Expired",
            Self::TriggerPending => "TriggerPending",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "New" => Ok(Self::New),
            "PartiallyFilled" => Ok(Self::PartiallyFilled),
            "Filled" => Ok(Self::Filled),
            "Cancelled" => Ok(Self::Cancelled),
            "Expired" => Ok(Self::Expired),
            "TriggerPending" => Ok(Self::TriggerPending),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderStatus literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { New, PartiallyFilled, Filled, Cancelled, Expired, TriggerPending });
    }
}

// ts: BackpackOrder
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackOrder {
    pub id: String,
    pub clientId: Option<f64>,
    pub symbol: String,
    pub side: BackpackOrderSide,
    pub orderType: BackpackOrderOrderType,
    pub price: String,
    pub triggerPrice: Option<String>,
    pub triggerBy: Option<Option<BackpackOrderTriggerBy>>,
    pub quantity: String,
    pub triggerQuantity: Option<String>,
    pub quoteQuantity: Option<String>,
    pub timeInForce: BackpackOrderTimeInForce,
    pub selfTradePrevention: BackpackOrderSelfTradePrevention,
    pub status: BackpackOrderStatus,
    pub createdAt: f64,
    pub executedQuantity: Option<String>,
    pub executedQuoteQuantity: Option<String>,
    pub reduceOnly: Option<bool>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackOrder")]
struct BackpackOrderWire {
    id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    clientId: Option<f64>,
    symbol: String,
    side: BackpackOrderSide,
    orderType: BackpackOrderOrderType,
    price: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerBy: Option<Option<BackpackOrderTriggerBy>>,
    quantity: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    quoteQuantity: Option<String>,
    timeInForce: BackpackOrderTimeInForce,
    selfTradePrevention: BackpackOrderSelfTradePrevention,
    status: BackpackOrderStatus,
    createdAt: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    executedQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    executedQuoteQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    reduceOnly: Option<bool>,
}
impl Serialize for BackpackOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackOrderWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackOrder {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackOrderWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackOrder {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, clientId, symbol, side, orderType, price, triggerPrice, triggerBy, quantity, triggerQuantity, quoteQuantity, timeInForce, selfTradePrevention, status, createdAt, executedQuantity, executedQuoteQuantity, reduceOnly });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestSide {
    Bid,
    Ask,
}
impl Serialize for BackpackOrderRequestSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestSide {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Bid" => Ok(Self::Bid),
            "Ask" => Ok(Self::Ask),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestSide literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Bid, Ask });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestOrderType {
    Limit,
    Market,
}
impl Serialize for BackpackOrderRequestOrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Limit => "Limit",
            Self::Market => "Market",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Limit" => Ok(Self::Limit),
            "Market" => Ok(Self::Market),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestOrderType literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestTimeInForce {
    Gtc,
    Ioc,
    Fok,
    Po,
}
impl Serialize for BackpackOrderRequestTimeInForce {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Gtc => "GTC",
            Self::Ioc => "IOC",
            Self::Fok => "FOK",
            Self::Po => "PO",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "GTC" => Ok(Self::Gtc),
            "IOC" => Ok(Self::Ioc),
            "FOK" => Ok(Self::Fok),
            "PO" => Ok(Self::Po),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestTimeInForce literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestTimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Gtc, Ioc, Fok, Po });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestSelfTradePrevention {
    RejectTaker,
    RejectMaker,
    RejectBoth,
    Allow,
}
impl Serialize for BackpackOrderRequestSelfTradePrevention {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::RejectTaker => "RejectTaker",
            Self::RejectMaker => "RejectMaker",
            Self::RejectBoth => "RejectBoth",
            Self::Allow => "Allow",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestSelfTradePrevention {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "RejectTaker" => Ok(Self::RejectTaker),
            "RejectMaker" => Ok(Self::RejectMaker),
            "RejectBoth" => Ok(Self::RejectBoth),
            "Allow" => Ok(Self::Allow),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestSelfTradePrevention literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestSelfTradePrevention {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { RejectTaker, RejectMaker, RejectBoth, Allow });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestTriggerBy {
    MarkPrice,
    IndexPrice,
    LastPrice,
}
impl Serialize for BackpackOrderRequestTriggerBy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::MarkPrice => "MarkPrice",
            Self::IndexPrice => "IndexPrice",
            Self::LastPrice => "LastPrice",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestTriggerBy {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "MarkPrice" => Ok(Self::MarkPrice),
            "IndexPrice" => Ok(Self::IndexPrice),
            "LastPrice" => Ok(Self::LastPrice),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestTriggerBy literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestTriggerBy {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { MarkPrice, IndexPrice, LastPrice });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestTakeProfitTriggerBy {
    MarkPrice,
    IndexPrice,
    LastPrice,
}
impl Serialize for BackpackOrderRequestTakeProfitTriggerBy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::MarkPrice => "MarkPrice",
            Self::IndexPrice => "IndexPrice",
            Self::LastPrice => "LastPrice",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestTakeProfitTriggerBy {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "MarkPrice" => Ok(Self::MarkPrice),
            "IndexPrice" => Ok(Self::IndexPrice),
            "LastPrice" => Ok(Self::LastPrice),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestTakeProfitTriggerBy literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestTakeProfitTriggerBy {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { MarkPrice, IndexPrice, LastPrice });
    }
}

// ts: BackpackOrderRequest (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackOrderRequestStopLossTriggerBy {
    MarkPrice,
    IndexPrice,
    LastPrice,
}
impl Serialize for BackpackOrderRequestStopLossTriggerBy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::MarkPrice => "MarkPrice",
            Self::IndexPrice => "IndexPrice",
            Self::LastPrice => "LastPrice",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequestStopLossTriggerBy {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "MarkPrice" => Ok(Self::MarkPrice),
            "IndexPrice" => Ok(Self::IndexPrice),
            "LastPrice" => Ok(Self::LastPrice),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackOrderRequestStopLossTriggerBy literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackOrderRequestStopLossTriggerBy {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { MarkPrice, IndexPrice, LastPrice });
    }
}

// ts: BackpackOrderRequest
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackOrderRequest {
    pub symbol: String,
    pub side: BackpackOrderRequestSide,
    pub orderType: BackpackOrderRequestOrderType,
    pub price: Option<String>,
    pub triggerPrice: Option<String>,
    pub quantity: Option<String>,
    pub triggerQuantity: Option<String>,
    pub quoteQuantity: Option<String>,
    pub timeInForce: Option<BackpackOrderRequestTimeInForce>,
    pub selfTradePrevention: Option<BackpackOrderRequestSelfTradePrevention>,
    pub clientId: Option<f64>,
    pub postOnly: Option<bool>,
    pub reduceOnly: Option<bool>,
    pub triggerBy: Option<BackpackOrderRequestTriggerBy>,
    pub takeProfitTriggerPrice: Option<String>,
    pub takeProfitLimitPrice: Option<String>,
    pub takeProfitTriggerBy: Option<BackpackOrderRequestTakeProfitTriggerBy>,
    pub stopLossTriggerPrice: Option<String>,
    pub stopLossLimitPrice: Option<String>,
    pub stopLossTriggerBy: Option<BackpackOrderRequestStopLossTriggerBy>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackOrderRequest")]
struct BackpackOrderRequestWire {
    symbol: String,
    side: BackpackOrderRequestSide,
    orderType: BackpackOrderRequestOrderType,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    price: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    quantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    quoteQuantity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    timeInForce: Option<BackpackOrderRequestTimeInForce>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    selfTradePrevention: Option<BackpackOrderRequestSelfTradePrevention>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    clientId: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    postOnly: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    reduceOnly: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    triggerBy: Option<BackpackOrderRequestTriggerBy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    takeProfitTriggerPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    takeProfitLimitPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    takeProfitTriggerBy: Option<BackpackOrderRequestTakeProfitTriggerBy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    stopLossTriggerPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    stopLossLimitPrice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    stopLossTriggerBy: Option<BackpackOrderRequestStopLossTriggerBy>,
}
impl Serialize for BackpackOrderRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackOrderRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackOrderRequest {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackOrderRequestWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackOrderRequest {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, side, orderType, price, triggerPrice, quantity, triggerQuantity, quoteQuantity, timeInForce, selfTradePrevention, clientId, postOnly, reduceOnly, triggerBy, takeProfitTriggerPrice, takeProfitLimitPrice, takeProfitTriggerBy, stopLossTriggerPrice, stopLossLimitPrice, stopLossTriggerBy });
    }
}

// ts: BackpackFill (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackFillSide {
    Bid,
    Ask,
}
impl Serialize for BackpackFillSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackFillSide {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Bid" => Ok(Self::Bid),
            "Ask" => Ok(Self::Ask),
            _ => Err(serde::de::Error::custom("unknown BackpackFillSide literal")),
        }
    }
}
impl JsNumberOutput for BackpackFillSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Bid, Ask });
    }
}

// ts: BackpackFill
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackFill {
    pub clientId: Option<f64>,
    pub orderId: String,
    pub symbol: String,
    pub side: BackpackFillSide,
    pub price: String,
    pub quantity: String,
    pub fee: String,
    pub feeSymbol: String,
    pub tradeId: f64,
    pub timestamp: String,
    pub isMaker: bool,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackFill")]
struct BackpackFillWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    clientId: Option<f64>,
    orderId: String,
    symbol: String,
    side: BackpackFillSide,
    price: String,
    quantity: String,
    fee: String,
    feeSymbol: String,
    tradeId: f64,
    timestamp: String,
    isMaker: bool,
}
impl Serialize for BackpackFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackFillWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackFill {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackFillWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackFill {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { clientId, orderId, symbol, side, price, quantity, fee, feeSymbol, tradeId, timestamp, isMaker });
    }
}

// ts: BackpackCollateral
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackCollateralCollateralItem {
    pub symbol: String,
    pub assetMarkPrice: String,
    pub availableQuantity: String,
    pub balanceNotional: String,
    pub collateralValue: String,
    pub collateralWeight: String,
    pub lendQuantity: String,
    pub openOrderQuantity: String,
    pub totalQuantity: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackCollateralCollateralItem")]
struct BackpackCollateralCollateralItemWire {
    symbol: String,
    assetMarkPrice: String,
    availableQuantity: String,
    balanceNotional: String,
    collateralValue: String,
    collateralWeight: String,
    lendQuantity: String,
    openOrderQuantity: String,
    totalQuantity: String,
}
impl Serialize for BackpackCollateralCollateralItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackCollateralCollateralItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackCollateralCollateralItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackCollateralCollateralItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackCollateralCollateralItem {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, assetMarkPrice, availableQuantity, balanceNotional, collateralValue, collateralWeight, lendQuantity, openOrderQuantity, totalQuantity });
    }
}

// ts: BackpackCollateral
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackCollateral {
    pub assetsValue: String,
    pub borrowLiability: String,
    pub collateral: Vec<BackpackCollateralCollateralItem>,
    pub imf: String,
    pub liabilitiesValue: String,
    pub marginFraction: String,
    pub mmf: String,
    pub netEquity: String,
    pub netEquityAvailable: String,
    pub netEquityLocked: String,
    pub netExposureFutures: String,
    pub pnlUnrealized: String,
    pub unsettledEquity: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackCollateral")]
struct BackpackCollateralWire {
    assetsValue: String,
    borrowLiability: String,
    collateral: Vec<BackpackCollateralCollateralItem>,
    imf: String,
    liabilitiesValue: String,
    marginFraction: String,
    mmf: String,
    netEquity: String,
    netEquityAvailable: String,
    netEquityLocked: String,
    netExposureFutures: String,
    pnlUnrealized: String,
    unsettledEquity: String,
}
impl Serialize for BackpackCollateral {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackCollateralWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackCollateral {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackCollateralWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackCollateral {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { assetsValue, borrowLiability, collateral, imf, liabilitiesValue, marginFraction, mmf, netEquity, netEquityAvailable, netEquityLocked, netExposureFutures, pnlUnrealized, unsettledEquity });
    }
}

// ts: BackpackPosition
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackPosition {
    pub symbol: String,
    pub positionId: String,
    pub netQuantity: String,
    pub netExposureQuantity: String,
    pub netExposureNotional: String,
    pub netCost: String,
    pub entryPrice: String,
    pub markPrice: String,
    pub breakEvenPrice: String,
    pub estLiquidationPrice: String,
    pub pnlUnrealized: String,
    pub pnlRealized: String,
    pub cumulativeFundingPayment: String,
    pub cumulativeInterest: String,
    pub imf: String,
    pub mmf: String,
    pub userId: f64,
    pub subaccountId: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackPosition")]
struct BackpackPositionWire {
    symbol: String,
    positionId: String,
    netQuantity: String,
    netExposureQuantity: String,
    netExposureNotional: String,
    netCost: String,
    entryPrice: String,
    markPrice: String,
    breakEvenPrice: String,
    estLiquidationPrice: String,
    pnlUnrealized: String,
    pnlRealized: String,
    cumulativeFundingPayment: String,
    cumulativeInterest: String,
    imf: String,
    mmf: String,
    userId: f64,
    #[serde(deserialize_with = "required_nullable")]
    subaccountId: Option<String>,
}
impl Serialize for BackpackPosition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackPositionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackPosition {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackPositionWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackPosition {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, positionId, netQuantity, netExposureQuantity, netExposureNotional, netCost, entryPrice, markPrice, breakEvenPrice, estLiquidationPrice, pnlUnrealized, pnlRealized, cumulativeFundingPayment, cumulativeInterest, imf, mmf, userId, subaccountId });
    }
}

// ts: BackpackWsSubscribeMessage (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsSubscribeMessageMethod {
    Subscribe,
}
impl Serialize for BackpackWsSubscribeMessageMethod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Subscribe => "SUBSCRIBE",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsSubscribeMessageMethod {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "SUBSCRIBE" => Ok(Self::Subscribe),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsSubscribeMessageMethod literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsSubscribeMessageMethod {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Subscribe });
    }
}

// ts: BackpackWsSubscribeMessage
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsSubscribeMessage {
    pub method: BackpackWsSubscribeMessageMethod,
    pub params: Vec<String>,
    pub signature: Option<BackpackWsSignature>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsSubscribeMessage")]
struct BackpackWsSubscribeMessageWire {
    method: BackpackWsSubscribeMessageMethod,
    params: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    signature: Option<BackpackWsSignature>,
}
impl Serialize for BackpackWsSubscribeMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsSubscribeMessageWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsSubscribeMessage {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsSubscribeMessageWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsSubscribeMessage {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { method, params, signature });
    }
}

// ts: BackpackWsUnsubscribeMessage (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsUnsubscribeMessageMethod {
    Unsubscribe,
}
impl Serialize for BackpackWsUnsubscribeMessageMethod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Unsubscribe => "UNSUBSCRIBE",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsUnsubscribeMessageMethod {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "UNSUBSCRIBE" => Ok(Self::Unsubscribe),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsUnsubscribeMessageMethod literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsUnsubscribeMessageMethod {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Unsubscribe });
    }
}

// ts: BackpackWsUnsubscribeMessage
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsUnsubscribeMessage {
    pub method: BackpackWsUnsubscribeMessageMethod,
    pub params: Vec<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsUnsubscribeMessage")]
struct BackpackWsUnsubscribeMessageWire {
    method: BackpackWsUnsubscribeMessageMethod,
    params: Vec<String>,
}
impl Serialize for BackpackWsUnsubscribeMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsUnsubscribeMessageWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsUnsubscribeMessage {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsUnsubscribeMessageWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsUnsubscribeMessage {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { method, params });
    }
}

// ts: BackpackWsSignature
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsSignature {
    pub key: String,
    pub signature: String,
    pub timestamp: f64,
    pub window: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsSignature")]
struct BackpackWsSignatureWire {
    key: String,
    signature: String,
    timestamp: f64,
    window: f64,
}
impl Serialize for BackpackWsSignature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsSignatureWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsSignature {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsSignatureWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsSignature {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { key, signature, timestamp, window });
    }
}

// ts: BackpackWsTickerUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsTickerUpdateE {
    Ticker,
}
impl Serialize for BackpackWsTickerUpdateE {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Ticker => "ticker",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsTickerUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "ticker" => Ok(Self::Ticker),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsTickerUpdateE literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsTickerUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Ticker });
    }
}

// ts: BackpackWsTickerUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsTickerUpdate {
    pub e: BackpackWsTickerUpdateE,
    pub s: String,
    pub p: String,
    pub P: String,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub v: String,
    pub q: String,
    pub E: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsTickerUpdate")]
struct BackpackWsTickerUpdateWire {
    e: BackpackWsTickerUpdateE,
    s: String,
    p: String,
    P: String,
    o: String,
    h: String,
    l: String,
    c: String,
    v: String,
    q: String,
    E: f64,
}
impl Serialize for BackpackWsTickerUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsTickerUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsTickerUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsTickerUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsTickerUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, s, p, P, o, h, l, c, v, q, E });
    }
}

// ts: BackpackWsDepthUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsDepthUpdateE {
    Depth,
}
impl Serialize for BackpackWsDepthUpdateE {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Depth => "depth",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsDepthUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "depth" => Ok(Self::Depth),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsDepthUpdateE literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsDepthUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Depth });
    }
}

// ts: BackpackWsDepthUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsDepthUpdate {
    pub e: BackpackWsDepthUpdateE,
    pub s: String,
    pub b: Vec<(String, String)>,
    pub a: Vec<(String, String)>,
    pub E: f64,
    pub T: Option<f64>,
    pub U: Option<f64>,
    pub u: Option<f64>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsDepthUpdate")]
struct BackpackWsDepthUpdateWire {
    e: BackpackWsDepthUpdateE,
    s: String,
    b: Vec<(String, String)>,
    a: Vec<(String, String)>,
    E: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    T: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    U: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    u: Option<f64>,
}
impl Serialize for BackpackWsDepthUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsDepthUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsDepthUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsDepthUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsDepthUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, s, b, a, E, T, U, u });
    }
}

// ts: BackpackWsTradeUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsTradeUpdateE {
    Trade,
}
impl Serialize for BackpackWsTradeUpdateE {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Trade => "trade",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsTradeUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "trade" => Ok(Self::Trade),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsTradeUpdateE literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsTradeUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Trade });
    }
}

// ts: BackpackWsTradeUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsTradeUpdate {
    pub e: BackpackWsTradeUpdateE,
    pub s: String,
    pub i: f64,
    pub p: String,
    pub q: String,
    pub m: bool,
    pub E: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsTradeUpdate")]
struct BackpackWsTradeUpdateWire {
    e: BackpackWsTradeUpdateE,
    s: String,
    i: f64,
    p: String,
    q: String,
    m: bool,
    E: f64,
}
impl Serialize for BackpackWsTradeUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsTradeUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsTradeUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsTradeUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsTradeUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, s, i, p, q, m, E });
    }
}

// ts: BackpackWsKlineUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsKlineUpdateE {
    Kline,
}
impl Serialize for BackpackWsKlineUpdateE {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Kline => "kline",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsKlineUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "kline" => Ok(Self::Kline),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsKlineUpdateE literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsKlineUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Kline });
    }
}

// ts: BackpackWsKlineUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsKlineUpdate {
    pub e: BackpackWsKlineUpdateE,
    pub s: String,
    pub i: Option<String>,
    pub t: String,
    pub T: String,
    pub o: Option<String>,
    pub h: Option<String>,
    pub l: Option<String>,
    pub c: Option<String>,
    pub v: Option<String>,
    pub n: f64,
    pub X: bool,
    pub E: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsKlineUpdate")]
struct BackpackWsKlineUpdateWire {
    e: BackpackWsKlineUpdateE,
    s: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    i: Option<String>,
    t: String,
    T: String,
    #[serde(deserialize_with = "required_nullable")]
    o: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    h: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    l: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    c: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    v: Option<String>,
    n: f64,
    X: bool,
    E: f64,
}
impl Serialize for BackpackWsKlineUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsKlineUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsKlineUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsKlineUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsKlineUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, s, i, t, T, o, h, l, c, v, n, X, E });
    }
}

// ts: BackpackWsBookTickerUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsBookTickerUpdateE {
    BookTicker,
}
impl Serialize for BackpackWsBookTickerUpdateE {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::BookTicker => "bookTicker",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsBookTickerUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "bookTicker" => Ok(Self::BookTicker),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsBookTickerUpdateE literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsBookTickerUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { BookTicker });
    }
}

// ts: BackpackWsBookTickerUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsBookTickerUpdate {
    pub e: BackpackWsBookTickerUpdateE,
    pub s: String,
    pub b: String,
    pub B: String,
    pub a: String,
    pub A: String,
    pub E: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsBookTickerUpdate")]
struct BackpackWsBookTickerUpdateWire {
    e: BackpackWsBookTickerUpdateE,
    s: String,
    b: String,
    B: String,
    a: String,
    A: String,
    E: f64,
}
impl Serialize for BackpackWsBookTickerUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsBookTickerUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsBookTickerUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsBookTickerUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsBookTickerUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, s, b, B, a, A, E });
    }
}

// ts: BackpackWsOrderUpdate (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackWsOrderUpdateS {
    Bid,
    Ask,
}
impl Serialize for BackpackWsOrderUpdateS {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackWsOrderUpdateS {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "Bid" => Ok(Self::Bid),
            "Ask" => Ok(Self::Ask),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackWsOrderUpdateS literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackWsOrderUpdateS {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Bid, Ask });
    }
}

// ts: BackpackWsOrderUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsOrderUpdate {
    pub e: String,
    pub i: String,
    pub c: Option<String>,
    pub E: f64,
    pub T: f64,
    pub X: String,
    pub s: String,
    pub o: String,
    pub S: BackpackWsOrderUpdateS,
    pub p: Option<String>,
    pub P: Option<String>,
    pub q: String,
    pub Q: Option<String>,
    pub l: Option<String>,
    pub L: Option<String>,
    pub z: String,
    pub Z: Option<String>,
    pub N: Option<String>,
    pub n: Option<String>,
    pub f: Option<String>,
    pub r: Option<bool>,
    pub t: Option<Option<f64>>,
    pub m: Option<bool>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsOrderUpdate")]
struct BackpackWsOrderUpdateWire {
    e: String,
    i: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    c: Option<String>,
    E: f64,
    T: f64,
    X: String,
    s: String,
    o: String,
    S: BackpackWsOrderUpdateS,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    p: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    P: Option<String>,
    q: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    Q: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    l: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    L: Option<String>,
    z: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    Z: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    N: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    n: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    f: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    r: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    t: Option<Option<f64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    m: Option<bool>,
}
impl Serialize for BackpackWsOrderUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsOrderUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsOrderUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsOrderUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsOrderUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e, i, c, E, T, X, s, o, S, p, P, q, Q, l, L, z, Z, N, n, f, r, t, m });
    }
}

// ts: BackpackWsPositionUpdate
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackWsPositionUpdate {
    pub i: f64,
    pub s: String,
    pub b: String,
    pub B: Option<String>,
    pub M: String,
    pub n: String,
    pub Q: String,
    pub q: String,
    pub P: String,
    pub p: String,
    pub l: String,
    pub m: String,
    pub f: String,
    pub E: f64,
    pub T: Option<f64>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackWsPositionUpdate")]
struct BackpackWsPositionUpdateWire {
    i: f64,
    s: String,
    b: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    B: Option<String>,
    M: String,
    n: String,
    Q: String,
    q: String,
    P: String,
    p: String,
    l: String,
    m: String,
    f: String,
    E: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    T: Option<f64>,
}
impl Serialize for BackpackWsPositionUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackWsPositionUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackWsPositionUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackWsPositionUpdateWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackWsPositionUpdate {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { i, s, b, B, M, n, Q, q, P, p, l, m, f, E, T });
    }
}

// ts: BackpackError
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackError {
    pub code: f64,
    pub msg: String,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackError")]
struct BackpackErrorWire {
    code: f64,
    msg: String,
}
impl Serialize for BackpackError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackErrorWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackError {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackErrorWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackError {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg });
    }
}

// ts: BackpackApiResponse
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackApiResponse<T> {
    pub data: Option<T>,
    pub error: Option<BackpackError>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackApiResponse")]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
struct BackpackApiResponseWire<T> {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    data: Option<T>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    error: Option<BackpackError>,
}
impl<T: Serialize + JsNumberOutput> Serialize for BackpackApiResponse<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackApiResponseWire::serialize(self, serializer)
    }
}
impl<'de, T: Deserialize<'de> + Serialize + JsNumberOutput> Deserialize<'de>
    for BackpackApiResponse<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackApiResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl<T: Serialize + JsNumberOutput> JsNumberOutput for BackpackApiResponse<T> {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { data, error });
    }
}

// ts: BackpackTime
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackTime {
    pub serverTime: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackTime")]
struct BackpackTimeWire {
    serverTime: f64,
}
impl Serialize for BackpackTime {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackTimeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackTime {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackTimeWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackTime {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { serverTime });
    }
}

// ts: BackpackStatus (inline literal union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpackStatusStatus {
    Normal,
    Maintenance,
}
impl Serialize for BackpackStatusStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Normal => "normal",
            Self::Maintenance => "maintenance",
        })
    }
}
impl<'de> Deserialize<'de> for BackpackStatusStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        match value.as_str() {
            "normal" => Ok(Self::Normal),
            "maintenance" => Ok(Self::Maintenance),
            _ => Err(serde::de::Error::custom(
                "unknown BackpackStatusStatus literal",
            )),
        }
    }
}
impl JsNumberOutput for BackpackStatusStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Normal, Maintenance });
    }
}

// ts: BackpackStatus
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackStatus {
    pub status: BackpackStatusStatus,
    pub msg: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackStatus")]
struct BackpackStatusWire {
    status: BackpackStatusStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    msg: Option<String>,
}
impl Serialize for BackpackStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackStatusWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackStatusWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackStatus {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { status, msg });
    }
}

// ts: BackpackAccount
#[allow(non_snake_case)] // Preserve case-sensitive source field identities (p/P, q/Q, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct BackpackAccount {
    pub autoBorrowSettlements: bool,
    pub autoLend: bool,
    pub autoRealizePnl: bool,
    pub autoRepayBorrows: bool,
    pub borrowLimit: String,
    pub futuresMakerFee: String,
    pub futuresTakerFee: String,
    pub leverageLimit: String,
    pub limitOrders: f64,
    pub liquidating: bool,
    pub positionLimit: String,
    pub spotMakerFee: String,
    pub spotTakerFee: String,
    pub triggerOrders: f64,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
#[serde(remote = "BackpackAccount")]
struct BackpackAccountWire {
    autoBorrowSettlements: bool,
    autoLend: bool,
    autoRealizePnl: bool,
    autoRepayBorrows: bool,
    borrowLimit: String,
    futuresMakerFee: String,
    futuresTakerFee: String,
    leverageLimit: String,
    limitOrders: f64,
    liquidating: bool,
    positionLimit: String,
    spotMakerFee: String,
    spotTakerFee: String,
    triggerOrders: f64,
}
impl Serialize for BackpackAccount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BackpackAccountWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BackpackAccount {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                BackpackAccountWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BackpackAccount {
    #[allow(non_snake_case)]
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { autoBorrowSettlements, autoLend, autoRealizePnl, autoRepayBorrows, borrowLimit, futuresMakerFee, futuresTakerFee, leverageLimit, limitOrders, liquidating, positionLimit, spotMakerFee, spotTakerFee, triggerOrders });
    }
}
