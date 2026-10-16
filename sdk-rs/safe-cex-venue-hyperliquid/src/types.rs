//! Typed wire models from hyperliquid.types.ts; JS numbers remain binary64.
use serde::{Deserialize, Serialize};

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
fn deserialize_required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(decoder)
}

pub const BASE_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "https://api.hyperliquid.xyz"),
    ("TESTNET", "https://api.hyperliquid-testnet.xyz"),
];

pub const WSS_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "wss://api-ui.hyperliquid.xyz/ws"),
    ("TESTNET", "wss://api.hyperliquid-testnet.xyz/ws"),
];

pub const ENDPOINTS: &[(&str, &str)] = &[("INFO", "/info"), ("EXCHANGE", "/exchange")];

pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &["/exchange"];

pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &["/info"];

pub const INFO_TYPES: &[(&str, &str)] = &[
    ("ALL_MIDS", "allMids"),
    ("META", "meta"),
    ("OPEN_ORDERS", "openOrders"),
    ("FRONTEND_OPEN_ORDERS", "frontendOpenOrders"),
    ("USER_FILLS", "userFills"),
    ("USER_FILLS_BY_TIME", "userFillsByTime"),
    ("HISTORICAL_ORDERS", "historicalOrders"),
    ("USER_RATE_LIMIT", "userRateLimit"),
    ("ORDER_STATUS", "orderStatus"),
    ("L2_BOOK", "l2Book"),
    ("CANDLE_SNAPSHOT", "candleSnapshot"),
    ("PERPS_META_AND_ASSET_CTXS", "metaAndAssetCtxs"),
    ("PERPS_CLEARINGHOUSE_STATE", "clearinghouseState"),
    ("USER_FUNDING", "userFunding"),
    (
        "USER_NON_FUNDING_LEDGER_UPDATES",
        "userNonFundingLedgerUpdates",
    ),
    ("FUNDING_HISTORY", "fundingHistory"),
    ("SPOT_META", "spotMeta"),
    ("SPOT_CLEARINGHOUSE_STATE", "spotClearinghouseState"),
    ("SPOT_META_AND_ASSET_CTXS", "spotMetaAndAssetCtxs"),
    ("PERP_DEXS", "perpDexs"),
    ("USER_ROLE", "userRole"),
    ("USER_ABSTRACTION", "userAbstraction"),
];

pub const EXCHANGE_TYPES: &[(&str, &str)] = &[
    ("ORDER", "order"),
    ("CANCEL", "cancel"),
    ("CANCEL_BY_CLOID", "cancelByCloid"),
    ("SCHEDULE_CANCEL", "scheduleCancel"),
    ("MODIFY", "modify"),
    ("BATCH_MODIFY", "batchModify"),
    ("UPDATE_LEVERAGE", "updateLeverage"),
    ("UPDATE_ISOLATED_MARGIN", "updateIsolatedMargin"),
    ("USD_SEND", "usdSend"),
    ("SPOT_SEND", "spotSend"),
    ("WITHDRAW", "withdraw3"),
    ("SPOT_USER", "spotUser"),
    ("VAULT_TRANSFER", "vaultTransfer"),
    ("SET_REFERRER", "setReferrer"),
];

pub const WEBSOCKET: &[(&str, &str)] = &[
    ("MAINNET_URL", "wss://api-ui.hyperliquid.xyz/ws"),
    ("TESTNET_URL", "wss://api.hyperliquid-testnet.xyz/ws"),
];

#[derive(Debug, Clone, PartialEq)]
pub enum HyperliquidUserAbstractionMode {
    UnifiedAccount,
    PortfolioMargin,
    Disabled,
    Default,
    DexAbstraction,
    Standard,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "HyperliquidUserAbstractionMode")]
enum HyperliquidUserAbstractionModeWire {
    #[serde(rename = "unifiedAccount")]
    UnifiedAccount,
    #[serde(rename = "portfolioMargin")]
    PortfolioMargin,
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "dexAbstraction")]
    DexAbstraction,
    #[serde(rename = "standard")]
    Standard,
}

impl Serialize for HyperliquidUserAbstractionMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        HyperliquidUserAbstractionModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for HyperliquidUserAbstractionMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        HyperliquidUserAbstractionModeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tif {
    Alo,
    Ioc,
    Gtc,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Tif")]
enum TifWire {
    #[serde(rename = "Alo")]
    Alo,
    #[serde(rename = "Ioc")]
    Ioc,
    #[serde(rename = "Gtc")]
    Gtc,
}

impl Serialize for Tif {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TifWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Tif {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TifWire::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&text))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tpsl {
    Tp,
    Sl,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Tpsl")]
enum TpslWire {
    #[serde(rename = "tp")]
    Tp,
    #[serde(rename = "sl")]
    Sl,
}

impl Serialize for Tpsl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TpslWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Tpsl {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TpslWire::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&text))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitOrderType {
    pub tif: Tif,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "LimitOrderType")]
struct LimitOrderTypeWire {
    #[serde(rename = "tif")]
    pub tif: Tif,
}

impl Serialize for LimitOrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        LimitOrderTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for LimitOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = LimitOrderType;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                LimitOrderTypeWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerOrderType {
    pub trigger_px: f64,
    pub is_market: bool,
    pub tpsl: Tpsl,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TriggerOrderType")]
struct TriggerOrderTypeWire {
    #[serde(rename = "triggerPx")]
    pub trigger_px: f64,
    #[serde(rename = "isMarket")]
    pub is_market: bool,
    #[serde(rename = "tpsl")]
    pub tpsl: Tpsl,
}

impl Serialize for TriggerOrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TriggerOrderTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for TriggerOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = TriggerOrderType;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                TriggerOrderTypeWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Grouping {
    Na,
    NormalTpsl,
    PositionTpsl,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Grouping")]
enum GroupingWire {
    #[serde(rename = "na")]
    Na,
    #[serde(rename = "normalTpsl")]
    NormalTpsl,
    #[serde(rename = "positionTpsl")]
    PositionTpsl,
}

impl Serialize for Grouping {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GroupingWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Grouping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        GroupingWire::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&text))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderType {
    pub limit: Option<LimitOrderType>,
    pub trigger: Option<TriggerOrderType>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderType")]
struct OrderTypeWire {
    #[serde(
        rename = "limit",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<LimitOrderType>,
    #[serde(
        rename = "trigger",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger: Option<TriggerOrderType>,
}

impl Serialize for OrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderType;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderTypeWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type Cloid = String;

#[derive(Debug, Clone, PartialEq)]
pub enum OidOrCloid {
    Case0(f64),
    Case1(Cloid),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OidOrCloid", untagged)]
enum OidOrCloidWire {
    Case0(f64),
    Case1(Cloid),
}

impl Serialize for OidOrCloid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OidOrCloidWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OidOrCloid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(OidOrCloidWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

pub type AllMids = crate::json_models::AllMids;

#[derive(Debug, Clone, PartialEq)]
pub enum MetaUniverseItemMarginMode {
    StrictIsolated,
    NoCross,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MetaUniverseItemMarginMode")]
enum MetaUniverseItemMarginModeWire {
    #[serde(rename = "strictIsolated")]
    StrictIsolated,
    #[serde(rename = "noCross")]
    NoCross,
}

impl Serialize for MetaUniverseItemMarginMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MetaUniverseItemMarginModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for MetaUniverseItemMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        MetaUniverseItemMarginModeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetaUniverseItemGrowthMode {
    Enabled,
    Disabled,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MetaUniverseItemGrowthMode")]
enum MetaUniverseItemGrowthModeWire {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

impl Serialize for MetaUniverseItemGrowthMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MetaUniverseItemGrowthModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for MetaUniverseItemGrowthMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        MetaUniverseItemGrowthModeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetaUniverseItem {
    pub name: String,
    pub sz_decimals: f64,
    pub max_leverage: f64,
    pub only_isolated: bool,
    pub is_delisted: Option<bool>,
    pub margin_table_id: Option<f64>,
    pub margin_mode: Option<MetaUniverseItemMarginMode>,
    pub growth_mode: Option<MetaUniverseItemGrowthMode>,
    pub last_growth_mode_change_time: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MetaUniverseItem")]
struct MetaUniverseItemWire {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "szDecimals")]
    pub sz_decimals: f64,
    #[serde(rename = "maxLeverage")]
    pub max_leverage: f64,
    #[serde(rename = "onlyIsolated")]
    pub only_isolated: bool,
    #[serde(
        rename = "isDelisted",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_delisted: Option<bool>,
    #[serde(
        rename = "marginTableId",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_table_id: Option<f64>,
    #[serde(
        rename = "marginMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_mode: Option<MetaUniverseItemMarginMode>,
    #[serde(
        rename = "growthMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub growth_mode: Option<MetaUniverseItemGrowthMode>,
    #[serde(
        rename = "lastGrowthModeChangeTime",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_growth_mode_change_time: Option<String>,
}

impl Serialize for MetaUniverseItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MetaUniverseItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for MetaUniverseItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = MetaUniverseItem;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                MetaUniverseItemWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub universe: Vec<MetaUniverseItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Meta")]
struct MetaWire {
    #[serde(rename = "universe")]
    pub universe: Vec<MetaUniverseItem>,
}

impl Serialize for Meta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MetaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Meta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = Meta;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                MetaWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateAssetPositionsItemPositionCumFunding {
    pub all_time: String,
    pub since_change: String,
    pub since_open: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateAssetPositionsItemPositionCumFunding")]
struct ClearinghouseStateAssetPositionsItemPositionCumFundingWire {
    #[serde(rename = "allTime")]
    pub all_time: String,
    #[serde(rename = "sinceChange")]
    pub since_change: String,
    #[serde(rename = "sinceOpen")]
    pub since_open: String,
}

impl Serialize for ClearinghouseStateAssetPositionsItemPositionCumFunding {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateAssetPositionsItemPositionCumFundingWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateAssetPositionsItemPositionCumFunding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateAssetPositionsItemPositionCumFunding;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateAssetPositionsItemPositionCumFundingWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateAssetPositionsItemPositionLeverage {
    pub raw_usd: String,
    pub r#type: String,
    pub value: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateAssetPositionsItemPositionLeverage")]
struct ClearinghouseStateAssetPositionsItemPositionLeverageWire {
    #[serde(rename = "rawUsd")]
    pub raw_usd: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "value")]
    pub value: f64,
}

impl Serialize for ClearinghouseStateAssetPositionsItemPositionLeverage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateAssetPositionsItemPositionLeverageWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateAssetPositionsItemPositionLeverage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateAssetPositionsItemPositionLeverage;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateAssetPositionsItemPositionLeverageWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateAssetPositionsItemPosition {
    pub coin: String,
    pub cum_funding: ClearinghouseStateAssetPositionsItemPositionCumFunding,
    pub entry_px: String,
    pub leverage: ClearinghouseStateAssetPositionsItemPositionLeverage,
    pub liquidation_px: String,
    pub margin_used: String,
    pub max_leverage: f64,
    pub position_value: String,
    pub return_on_equity: String,
    pub szi: String,
    pub unrealized_pnl: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateAssetPositionsItemPosition")]
struct ClearinghouseStateAssetPositionsItemPositionWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "cumFunding")]
    pub cum_funding: ClearinghouseStateAssetPositionsItemPositionCumFunding,
    #[serde(rename = "entryPx")]
    pub entry_px: String,
    #[serde(rename = "leverage")]
    pub leverage: ClearinghouseStateAssetPositionsItemPositionLeverage,
    #[serde(rename = "liquidationPx")]
    pub liquidation_px: String,
    #[serde(rename = "marginUsed")]
    pub margin_used: String,
    #[serde(rename = "maxLeverage")]
    pub max_leverage: f64,
    #[serde(rename = "positionValue")]
    pub position_value: String,
    #[serde(rename = "returnOnEquity")]
    pub return_on_equity: String,
    #[serde(rename = "szi")]
    pub szi: String,
    #[serde(rename = "unrealizedPnl")]
    pub unrealized_pnl: String,
}

impl Serialize for ClearinghouseStateAssetPositionsItemPosition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateAssetPositionsItemPositionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateAssetPositionsItemPosition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateAssetPositionsItemPosition;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateAssetPositionsItemPositionWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateAssetPositionsItem {
    pub position: ClearinghouseStateAssetPositionsItemPosition,
    pub r#type: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateAssetPositionsItem")]
struct ClearinghouseStateAssetPositionsItemWire {
    #[serde(rename = "position")]
    pub position: ClearinghouseStateAssetPositionsItemPosition,
    #[serde(rename = "type")]
    pub r#type: String,
}

impl Serialize for ClearinghouseStateAssetPositionsItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateAssetPositionsItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateAssetPositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateAssetPositionsItem;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateAssetPositionsItemWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateCrossMarginSummary {
    pub account_value: String,
    pub total_margin_used: String,
    pub total_ntl_pos: String,
    pub total_raw_usd: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateCrossMarginSummary")]
struct ClearinghouseStateCrossMarginSummaryWire {
    #[serde(rename = "accountValue")]
    pub account_value: String,
    #[serde(rename = "totalMarginUsed")]
    pub total_margin_used: String,
    #[serde(rename = "totalNtlPos")]
    pub total_ntl_pos: String,
    #[serde(rename = "totalRawUsd")]
    pub total_raw_usd: String,
}

impl Serialize for ClearinghouseStateCrossMarginSummary {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateCrossMarginSummaryWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateCrossMarginSummary {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateCrossMarginSummary;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateCrossMarginSummaryWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateMarginSummary {
    pub account_value: String,
    pub total_margin_used: String,
    pub total_ntl_pos: String,
    pub total_raw_usd: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseStateMarginSummary")]
struct ClearinghouseStateMarginSummaryWire {
    #[serde(rename = "accountValue")]
    pub account_value: String,
    #[serde(rename = "totalMarginUsed")]
    pub total_margin_used: String,
    #[serde(rename = "totalNtlPos")]
    pub total_ntl_pos: String,
    #[serde(rename = "totalRawUsd")]
    pub total_raw_usd: String,
}

impl Serialize for ClearinghouseStateMarginSummary {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateMarginSummaryWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseStateMarginSummary {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseStateMarginSummary;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateMarginSummaryWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseState {
    pub asset_positions: Vec<ClearinghouseStateAssetPositionsItem>,
    pub cross_maintenance_margin_used: String,
    pub cross_margin_summary: ClearinghouseStateCrossMarginSummary,
    pub margin_summary: ClearinghouseStateMarginSummary,
    pub time: f64,
    pub withdrawable: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ClearinghouseState")]
struct ClearinghouseStateWire {
    #[serde(rename = "assetPositions")]
    pub asset_positions: Vec<ClearinghouseStateAssetPositionsItem>,
    #[serde(rename = "crossMaintenanceMarginUsed")]
    pub cross_maintenance_margin_used: String,
    #[serde(rename = "crossMarginSummary")]
    pub cross_margin_summary: ClearinghouseStateCrossMarginSummary,
    #[serde(rename = "marginSummary")]
    pub margin_summary: ClearinghouseStateMarginSummary,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "withdrawable")]
    pub withdrawable: String,
}

impl Serialize for ClearinghouseState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ClearinghouseStateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ClearinghouseState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ClearinghouseState;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ClearinghouseStateWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserFills {
    pub coin: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub time: f64,
    pub start_position: String,
    pub dir: String,
    pub closed_pnl: String,
    pub hash: String,
    pub oid: f64,
    pub crossed: bool,
    pub fee: String,
    pub tid: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserFills")]
struct UserFillsWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "startPosition")]
    pub start_position: String,
    #[serde(rename = "dir")]
    pub dir: String,
    #[serde(rename = "closedPnl")]
    pub closed_pnl: String,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "crossed")]
    pub crossed: bool,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "tid")]
    pub tid: f64,
}

impl Serialize for UserFills {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserFillsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserFills {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserFills;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserFillsWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseStatusCase0RestingOid {
    Case0(f64),
    Case1(String),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase0RestingOid", untagged)]
enum OrderResponseStatusCase0RestingOidWire {
    Case0(f64),
    Case1(String),
}

impl Serialize for OrderResponseStatusCase0RestingOid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase0RestingOidWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase0RestingOid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(OrderResponseStatusCase0RestingOidWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseStatusCase0Resting {
    pub oid: OrderResponseStatusCase0RestingOid,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase0Resting")]
struct OrderResponseStatusCase0RestingWire {
    #[serde(rename = "oid")]
    pub oid: OrderResponseStatusCase0RestingOid,
}

impl Serialize for OrderResponseStatusCase0Resting {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase0RestingWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase0Resting {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseStatusCase0Resting;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseStatusCase0RestingWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseStatusCase0 {
    pub resting: OrderResponseStatusCase0Resting,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase0")]
struct OrderResponseStatusCase0Wire {
    #[serde(rename = "resting")]
    pub resting: OrderResponseStatusCase0Resting,
}

impl Serialize for OrderResponseStatusCase0 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase0Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase0 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseStatusCase0;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseStatusCase0Wire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseStatusCase1FilledOid {
    Case0(f64),
    Case1(String),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase1FilledOid", untagged)]
enum OrderResponseStatusCase1FilledOidWire {
    Case0(f64),
    Case1(String),
}

impl Serialize for OrderResponseStatusCase1FilledOid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase1FilledOidWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase1FilledOid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(OrderResponseStatusCase1FilledOidWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseStatusCase1Filled {
    pub oid: OrderResponseStatusCase1FilledOid,
    pub total_sz: Option<String>,
    pub avg_px: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase1Filled")]
struct OrderResponseStatusCase1FilledWire {
    #[serde(rename = "oid")]
    pub oid: OrderResponseStatusCase1FilledOid,
    #[serde(
        rename = "totalSz",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub total_sz: Option<String>,
    #[serde(
        rename = "avgPx",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub avg_px: Option<String>,
}

impl Serialize for OrderResponseStatusCase1Filled {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase1FilledWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase1Filled {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseStatusCase1Filled;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseStatusCase1FilledWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseStatusCase1 {
    pub filled: OrderResponseStatusCase1Filled,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase1")]
struct OrderResponseStatusCase1Wire {
    #[serde(rename = "filled")]
    pub filled: OrderResponseStatusCase1Filled,
}

impl Serialize for OrderResponseStatusCase1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase1Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseStatusCase1;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseStatusCase1Wire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseStatusCase2 {
    pub error: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase2")]
struct OrderResponseStatusCase2Wire {
    #[serde(rename = "error")]
    pub error: String,
}

impl Serialize for OrderResponseStatusCase2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase2Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseStatusCase2;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseStatusCase2Wire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseStatusCase3 {
    WaitingForFill,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase3")]
enum OrderResponseStatusCase3Wire {
    #[serde(rename = "waitingForFill")]
    WaitingForFill,
}

impl Serialize for OrderResponseStatusCase3 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase3Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase3 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        OrderResponseStatusCase3Wire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseStatusCase4 {
    WaitingForTrigger,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatusCase4")]
enum OrderResponseStatusCase4Wire {
    #[serde(rename = "waitingForTrigger")]
    WaitingForTrigger,
}

impl Serialize for OrderResponseStatusCase4 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusCase4Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatusCase4 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        OrderResponseStatusCase4Wire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseStatus {
    Case0(OrderResponseStatusCase0),
    Case1(OrderResponseStatusCase1),
    Case2(OrderResponseStatusCase2),
    Case3(OrderResponseStatusCase3),
    Case4(OrderResponseStatusCase4),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseStatus", untagged)]
enum OrderResponseStatusWire {
    Case0(OrderResponseStatusCase0),
    Case1(OrderResponseStatusCase1),
    Case2(OrderResponseStatusCase2),
    Case3(OrderResponseStatusCase3),
    Case4(OrderResponseStatusCase4),
}

impl Serialize for OrderResponseStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseStatusWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(OrderResponseStatusWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseResponseCase1Data {
    pub statuses: Vec<OrderResponseStatus>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseResponseCase1Data")]
struct OrderResponseResponseCase1DataWire {
    #[serde(rename = "statuses")]
    pub statuses: Vec<OrderResponseStatus>,
}

impl Serialize for OrderResponseResponseCase1Data {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseResponseCase1DataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseResponseCase1Data {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseResponseCase1Data;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseResponseCase1DataWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponseResponseCase1 {
    pub r#type: String,
    pub data: OrderResponseResponseCase1Data,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseResponseCase1")]
struct OrderResponseResponseCase1Wire {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "data")]
    pub data: OrderResponseResponseCase1Data,
}

impl Serialize for OrderResponseResponseCase1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseResponseCase1Wire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseResponseCase1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponseResponseCase1;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseResponseCase1Wire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderResponseResponse {
    Case0(String),
    Case1(OrderResponseResponseCase1),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponseResponse", untagged)]
enum OrderResponseResponseWire {
    Case0(String),
    Case1(OrderResponseResponseCase1),
}

impl Serialize for OrderResponseResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponseResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(OrderResponseResponseWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponse {
    pub status: String,
    pub response: OrderResponseResponse,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponse")]
struct OrderResponseWire {
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "response")]
    pub response: OrderResponseResponse,
}

impl Serialize for OrderResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderResponse;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderResponseWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsTrade {
    pub coin: String,
    pub side: String,
    pub px: String,
    pub sz: String,
    pub hash: String,
    pub time: f64,
    pub tid: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsTrade")]
struct WsTradeWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "tid")]
    pub tid: f64,
}

impl Serialize for WsTrade {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsTradeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsTrade {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsTrade;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsTradeWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsBook {
    pub coin: String,
    pub levels: (Vec<WsLevel>, Vec<WsLevel>),
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsBook")]
struct WsBookWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "levels")]
    pub levels: (Vec<WsLevel>, Vec<WsLevel>),
    #[serde(rename = "time")]
    pub time: f64,
}

impl Serialize for WsBook {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsBookWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsBook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsBook;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsBookWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsBbo {
    pub coin: String,
    pub bbo: (Option<WsLevel>, Option<WsLevel>),
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsBbo")]
struct WsBboWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "bbo")]
    pub bbo: (Option<WsLevel>, Option<WsLevel>),
    #[serde(rename = "time")]
    pub time: f64,
}

impl Serialize for WsBbo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsBboWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsBbo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsBbo;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsBboWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsLevel {
    pub px: String,
    pub sz: String,
    pub n: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsLevel")]
struct WsLevelWire {
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "n")]
    pub n: f64,
}

impl Serialize for WsLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsLevelWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsLevel;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsLevelWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsOrderOrder {
    pub coin: String,
    pub side: String,
    pub limit_px: String,
    pub sz: String,
    pub oid: f64,
    pub timestamp: f64,
    pub orig_sz: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsOrderOrder")]
struct WsOrderOrderWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "limitPx")]
    pub limit_px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
    #[serde(rename = "origSz")]
    pub orig_sz: String,
}

impl Serialize for WsOrderOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsOrderOrderWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsOrderOrder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsOrderOrder;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsOrderOrderWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsOrder {
    pub order: WsOrderOrder,
    pub status: String,
    pub status_timestamp: f64,
    pub user: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsOrder")]
struct WsOrderWire {
    #[serde(rename = "order")]
    pub order: WsOrderOrder,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "statusTimestamp")]
    pub status_timestamp: f64,
    #[serde(rename = "user")]
    pub user: String,
}

impl Serialize for WsOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsOrderWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsOrder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsOrder;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsOrderWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type WsUserEvent = crate::spot_user_event::WsUserEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct WsFill {
    pub coin: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub time: f64,
    pub start_position: String,
    pub dir: String,
    pub closed_pnl: String,
    pub hash: String,
    pub oid: f64,
    pub crossed: bool,
    pub fee: String,
    pub tid: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsFill")]
struct WsFillWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "startPosition")]
    pub start_position: String,
    #[serde(rename = "dir")]
    pub dir: String,
    #[serde(rename = "closedPnl")]
    pub closed_pnl: String,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "crossed")]
    pub crossed: bool,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "tid")]
    pub tid: f64,
}

impl Serialize for WsFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsFillWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsFill {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsFill;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsFillWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserFunding {
    pub time: f64,
    pub coin: String,
    pub usdc: String,
    pub szi: String,
    pub funding_rate: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserFunding")]
struct WsUserFundingWire {
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "usdc")]
    pub usdc: String,
    #[serde(rename = "szi")]
    pub szi: String,
    #[serde(rename = "fundingRate")]
    pub funding_rate: String,
}

impl Serialize for WsUserFunding {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserFundingWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserFunding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserFunding;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserFundingWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsLiquidation {
    pub lid: f64,
    pub liquidator: String,
    pub liquidated_user: String,
    pub liquidated_ntl_pos: String,
    pub liquidated_account_value: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsLiquidation")]
struct WsLiquidationWire {
    #[serde(rename = "lid")]
    pub lid: f64,
    #[serde(rename = "liquidator")]
    pub liquidator: String,
    #[serde(rename = "liquidated_user")]
    pub liquidated_user: String,
    #[serde(rename = "liquidated_ntl_pos")]
    pub liquidated_ntl_pos: String,
    #[serde(rename = "liquidated_account_value")]
    pub liquidated_account_value: String,
}

impl Serialize for WsLiquidation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsLiquidationWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsLiquidation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsLiquidation;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsLiquidationWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsNonUserCancel {
    pub coin: String,
    pub oid: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsNonUserCancel")]
struct WsNonUserCancelWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "oid")]
    pub oid: f64,
}

impl Serialize for WsNonUserCancel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsNonUserCancelWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsNonUserCancel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsNonUserCancel;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsNonUserCancelWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotClearinghouseBalance {
    pub coin: String,
    pub token: Option<f64>,
    pub hold: String,
    pub total: String,
    pub entry_ntl: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotClearinghouseBalance")]
struct SpotClearinghouseBalanceWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(
        rename = "token",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub token: Option<f64>,
    #[serde(rename = "hold")]
    pub hold: String,
    #[serde(rename = "total")]
    pub total: String,
    #[serde(
        rename = "entryNtl",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub entry_ntl: Option<String>,
}

impl Serialize for SpotClearinghouseBalance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotClearinghouseBalanceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotClearinghouseBalance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotClearinghouseBalance;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotClearinghouseBalanceWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotClearinghouseState {
    pub balances: Vec<SpotClearinghouseBalance>,
    pub token_to_available_after_maintenance: Option<Vec<(f64, String)>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotClearinghouseState")]
struct SpotClearinghouseStateWire {
    #[serde(rename = "balances")]
    pub balances: Vec<SpotClearinghouseBalance>,
    #[serde(
        rename = "tokenToAvailableAfterMaintenance",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub token_to_available_after_maintenance: Option<Vec<(f64, String)>>,
}

impl Serialize for SpotClearinghouseState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotClearinghouseStateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotClearinghouseState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotClearinghouseState;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotClearinghouseStateWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrontendOpenOrders {
    pub coin: String,
    pub is_position_tpsl: bool,
    pub is_trigger: bool,
    pub limit_px: String,
    pub oid: f64,
    pub order_type: String,
    pub orig_sz: String,
    pub reduce_only: bool,
    pub side: String,
    pub sz: String,
    pub timestamp: f64,
    pub trigger_condition: String,
    pub trigger_px: String,
    pub tif: Option<Option<Tif>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FrontendOpenOrders")]
struct FrontendOpenOrdersWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "isPositionTpsl")]
    pub is_position_tpsl: bool,
    #[serde(rename = "isTrigger")]
    pub is_trigger: bool,
    #[serde(rename = "limitPx")]
    pub limit_px: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "orderType")]
    pub order_type: String,
    #[serde(rename = "origSz")]
    pub orig_sz: String,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: bool,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
    #[serde(rename = "triggerCondition")]
    pub trigger_condition: String,
    #[serde(rename = "triggerPx")]
    pub trigger_px: String,
    #[serde(
        rename = "tif",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tif: Option<Option<Tif>>,
}

impl Serialize for FrontendOpenOrders {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FrontendOpenOrdersWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FrontendOpenOrders {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = FrontendOpenOrders;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                FrontendOpenOrdersWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type UserRateLimit = crate::json_models::UserRateLimit;

pub type OrderStatus = crate::json_models::OrderStatus;

#[derive(Debug, Clone, PartialEq)]
pub struct L2BookLevelsItem0Item {
    pub px: String,
    pub sz: String,
    pub n: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "L2BookLevelsItem0Item")]
struct L2BookLevelsItem0ItemWire {
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "n")]
    pub n: f64,
}

impl Serialize for L2BookLevelsItem0Item {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        L2BookLevelsItem0ItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for L2BookLevelsItem0Item {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = L2BookLevelsItem0Item;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                L2BookLevelsItem0ItemWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct L2BookLevelsItem1Item {
    pub px: String,
    pub sz: String,
    pub n: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "L2BookLevelsItem1Item")]
struct L2BookLevelsItem1ItemWire {
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "n")]
    pub n: f64,
}

impl Serialize for L2BookLevelsItem1Item {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        L2BookLevelsItem1ItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for L2BookLevelsItem1Item {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = L2BookLevelsItem1Item;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                L2BookLevelsItem1ItemWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct L2Book {
    pub levels: (Vec<L2BookLevelsItem0Item>, Vec<L2BookLevelsItem1Item>),
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "L2Book")]
struct L2BookWire {
    #[serde(rename = "levels")]
    pub levels: (Vec<L2BookLevelsItem0Item>, Vec<L2BookLevelsItem1Item>),
}

impl Serialize for L2Book {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        L2BookWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for L2Book {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = L2Book;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                L2BookWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandleSnapshot {
    pub t_upper: f64,
    pub c: String,
    pub h: String,
    pub i: String,
    pub l: String,
    pub n: f64,
    pub o: String,
    pub s: String,
    pub t: f64,
    pub v: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CandleSnapshot")]
struct CandleSnapshotWire {
    #[serde(rename = "T")]
    pub t_upper: f64,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "h")]
    pub h: String,
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "n")]
    pub n: f64,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "t")]
    pub t: f64,
    #[serde(rename = "v")]
    pub v: String,
}

impl Serialize for CandleSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CandleSnapshotWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CandleSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CandleSnapshot;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CandleSnapshotWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetCtx {
    pub day_ntl_vlm: String,
    pub day_base_vlm: Option<String>,
    pub funding: String,
    pub impact_pxs: (String, String),
    pub mark_px: String,
    pub mid_px: String,
    pub open_interest: String,
    pub oracle_px: String,
    pub premium: String,
    pub prev_day_px: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "AssetCtx")]
struct AssetCtxWire {
    #[serde(rename = "dayNtlVlm")]
    pub day_ntl_vlm: String,
    #[serde(
        rename = "dayBaseVlm",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub day_base_vlm: Option<String>,
    #[serde(rename = "funding")]
    pub funding: String,
    #[serde(rename = "impactPxs")]
    pub impact_pxs: (String, String),
    #[serde(rename = "markPx")]
    pub mark_px: String,
    #[serde(rename = "midPx")]
    pub mid_px: String,
    #[serde(rename = "openInterest")]
    pub open_interest: String,
    #[serde(rename = "oraclePx")]
    pub oracle_px: String,
    #[serde(rename = "premium")]
    pub premium: String,
    #[serde(rename = "prevDayPx")]
    pub prev_day_px: String,
}

impl Serialize for AssetCtx {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AssetCtxWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for AssetCtx {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = AssetCtx;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                AssetCtxWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetaAndAssetCtxs {
    pub meta: Meta,
    pub asset_ctxs: Vec<AssetCtx>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MetaAndAssetCtxs")]
struct MetaAndAssetCtxsWire {
    #[serde(rename = "meta")]
    pub meta: Meta,
    #[serde(rename = "assetCtxs")]
    pub asset_ctxs: Vec<AssetCtx>,
}

impl Serialize for MetaAndAssetCtxs {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MetaAndAssetCtxsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for MetaAndAssetCtxs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = MetaAndAssetCtxs;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                MetaAndAssetCtxsWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UserFundingDeltaType {
    Funding,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserFundingDeltaType")]
enum UserFundingDeltaTypeWire {
    #[serde(rename = "funding")]
    Funding,
}

impl Serialize for UserFundingDeltaType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserFundingDeltaTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserFundingDeltaType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        UserFundingDeltaTypeWire::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserFundingDelta {
    pub coin: String,
    pub funding_rate: String,
    pub szi: String,
    pub r#type: UserFundingDeltaType,
    pub usdc: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserFundingDelta")]
struct UserFundingDeltaWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "fundingRate")]
    pub funding_rate: String,
    #[serde(rename = "szi")]
    pub szi: String,
    #[serde(rename = "type")]
    pub r#type: UserFundingDeltaType,
    #[serde(rename = "usdc")]
    pub usdc: String,
}

impl Serialize for UserFundingDelta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserFundingDeltaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserFundingDelta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserFundingDelta;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserFundingDeltaWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserFundingEntry {
    pub delta: UserFundingDelta,
    pub hash: String,
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserFundingEntry")]
struct UserFundingEntryWire {
    #[serde(rename = "delta")]
    pub delta: UserFundingDelta,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "time")]
    pub time: f64,
}

impl Serialize for UserFundingEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserFundingEntryWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserFundingEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserFundingEntry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserFundingEntryWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type UserFunding = Vec<UserFundingEntry>;

#[derive(Debug, Clone, PartialEq)]
pub enum UserNonFundingLedgerDeltaType {
    Deposit,
    Withdraw,
    Transfer,
    Liquidation,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserNonFundingLedgerDeltaType")]
enum UserNonFundingLedgerDeltaTypeWire {
    #[serde(rename = "deposit")]
    Deposit,
    #[serde(rename = "withdraw")]
    Withdraw,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "liquidation")]
    Liquidation,
}

impl Serialize for UserNonFundingLedgerDeltaType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserNonFundingLedgerDeltaTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserNonFundingLedgerDeltaType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        UserNonFundingLedgerDeltaTypeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserNonFundingLedgerDelta {
    pub coin: String,
    pub r#type: UserNonFundingLedgerDeltaType,
    pub usdc: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserNonFundingLedgerDelta")]
struct UserNonFundingLedgerDeltaWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "type")]
    pub r#type: UserNonFundingLedgerDeltaType,
    #[serde(rename = "usdc")]
    pub usdc: String,
}

impl Serialize for UserNonFundingLedgerDelta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserNonFundingLedgerDeltaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserNonFundingLedgerDelta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserNonFundingLedgerDelta;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserNonFundingLedgerDeltaWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserNonFundingLedgerEntry {
    pub delta: UserNonFundingLedgerDelta,
    pub hash: String,
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserNonFundingLedgerEntry")]
struct UserNonFundingLedgerEntryWire {
    #[serde(rename = "delta")]
    pub delta: UserNonFundingLedgerDelta,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "time")]
    pub time: f64,
}

impl Serialize for UserNonFundingLedgerEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserNonFundingLedgerEntryWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserNonFundingLedgerEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserNonFundingLedgerEntry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserNonFundingLedgerEntryWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type UserNonFundingLedgerUpdates = Vec<UserNonFundingLedgerEntry>;

#[derive(Debug, Clone, PartialEq)]
pub struct FundingHistoryEntry {
    pub coin: String,
    pub funding_rate: String,
    pub premium: String,
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FundingHistoryEntry")]
struct FundingHistoryEntryWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "fundingRate")]
    pub funding_rate: String,
    #[serde(rename = "premium")]
    pub premium: String,
    #[serde(rename = "time")]
    pub time: f64,
}

impl Serialize for FundingHistoryEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FundingHistoryEntryWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for FundingHistoryEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = FundingHistoryEntry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                FundingHistoryEntryWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type FundingHistory = Vec<FundingHistoryEntry>;

#[derive(Debug, Clone, PartialEq)]
pub struct SpotTokenEvmContract {
    pub address: String,
    pub evm_extra_wei_decimals: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotTokenEvmContract")]
struct SpotTokenEvmContractWire {
    #[serde(rename = "address")]
    pub address: String,
    #[serde(rename = "evm_extra_wei_decimals")]
    pub evm_extra_wei_decimals: f64,
}

impl Serialize for SpotTokenEvmContract {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotTokenEvmContractWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotTokenEvmContract {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotTokenEvmContract;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotTokenEvmContractWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotToken {
    pub name: String,
    pub sz_decimals: f64,
    pub wei_decimals: f64,
    pub index: f64,
    pub token_id: String,
    pub is_canonical: bool,
    pub full_name: Option<Option<String>>,
    pub evm_contract: Option<Option<SpotTokenEvmContract>>,
    pub deployer_trading_fee_share: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotToken")]
struct SpotTokenWire {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "szDecimals")]
    pub sz_decimals: f64,
    #[serde(rename = "weiDecimals")]
    pub wei_decimals: f64,
    #[serde(rename = "index")]
    pub index: f64,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "isCanonical")]
    pub is_canonical: bool,
    #[serde(
        rename = "fullName",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub full_name: Option<Option<String>>,
    #[serde(
        rename = "evmContract",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub evm_contract: Option<Option<SpotTokenEvmContract>>,
    #[serde(
        rename = "deployerTradingFeeShare",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub deployer_trading_fee_share: Option<String>,
}

impl Serialize for SpotToken {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotTokenWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotToken {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotToken;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotTokenWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotMarket {
    pub name: String,
    pub tokens: (f64, f64),
    pub index: f64,
    pub is_canonical: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotMarket")]
struct SpotMarketWire {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "tokens")]
    pub tokens: (f64, f64),
    #[serde(rename = "index")]
    pub index: f64,
    #[serde(rename = "isCanonical")]
    pub is_canonical: bool,
}

impl Serialize for SpotMarket {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotMarketWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotMarket {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotMarket;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotMarketWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotMeta {
    pub tokens: Vec<SpotToken>,
    pub universe: Vec<SpotMarket>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotMeta")]
struct SpotMetaWire {
    #[serde(rename = "tokens")]
    pub tokens: Vec<SpotToken>,
    #[serde(rename = "universe")]
    pub universe: Vec<SpotMarket>,
}

impl Serialize for SpotMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotMetaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotMeta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotMeta;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotMetaWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotAssetCtx {
    pub day_ntl_vlm: String,
    pub mark_px: String,
    pub mid_px: Option<String>,
    pub prev_day_px: String,
    pub coin: Option<String>,
    pub circulating_supply: Option<String>,
    pub total_supply: Option<String>,
    pub day_base_vlm: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotAssetCtx")]
struct SpotAssetCtxWire {
    #[serde(rename = "dayNtlVlm")]
    pub day_ntl_vlm: String,
    #[serde(rename = "markPx")]
    pub mark_px: String,
    #[serde(rename = "midPx", deserialize_with = "deserialize_required_nullable")]
    pub mid_px: Option<String>,
    #[serde(rename = "prevDayPx")]
    pub prev_day_px: String,
    #[serde(
        rename = "coin",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub coin: Option<String>,
    #[serde(
        rename = "circulatingSupply",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub circulating_supply: Option<String>,
    #[serde(
        rename = "totalSupply",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub total_supply: Option<String>,
    #[serde(
        rename = "dayBaseVlm",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub day_base_vlm: Option<String>,
}

impl Serialize for SpotAssetCtx {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotAssetCtxWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotAssetCtx {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotAssetCtx;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotAssetCtxWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotMetaAndAssetCtxs {
    pub meta: SpotMeta,
    pub asset_ctxs: Vec<SpotAssetCtx>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "SpotMetaAndAssetCtxs")]
struct SpotMetaAndAssetCtxsWire {
    #[serde(rename = "meta")]
    pub meta: SpotMeta,
    #[serde(rename = "assetCtxs")]
    pub asset_ctxs: Vec<SpotAssetCtx>,
}

impl Serialize for SpotMetaAndAssetCtxs {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SpotMetaAndAssetCtxsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for SpotMetaAndAssetCtxs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = SpotMetaAndAssetCtxs;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SpotMetaAndAssetCtxsWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserOpenOrder {
    pub coin: String,
    pub limit_px: String,
    pub oid: f64,
    pub side: String,
    pub sz: String,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserOpenOrder")]
struct UserOpenOrderWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "limitPx")]
    pub limit_px: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
}

impl Serialize for UserOpenOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserOpenOrderWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for UserOpenOrder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = UserOpenOrder;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                UserOpenOrderWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type UserOpenOrders = Vec<UserOpenOrder>;

#[derive(Debug, Clone, PartialEq)]
pub struct OrderRequest {
    pub coin: String,
    pub is_buy: bool,
    pub sz: f64,
    pub limit_px: f64,
    pub order_type: OrderType,
    pub reduce_only: bool,
    pub cloid: Option<Cloid>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderRequest")]
struct OrderRequestWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "is_buy")]
    pub is_buy: bool,
    #[serde(rename = "sz")]
    pub sz: f64,
    #[serde(rename = "limit_px")]
    pub limit_px: f64,
    #[serde(rename = "order_type")]
    pub order_type: OrderType,
    #[serde(rename = "reduce_only")]
    pub reduce_only: bool,
    #[serde(
        rename = "cloid",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub cloid: Option<Cloid>,
}

impl Serialize for OrderRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderRequest;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderRequestWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderWire {
    pub a: f64,
    pub b: bool,
    pub p: String,
    pub s: String,
    pub r: bool,
    pub t: OrderType,
    pub c: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderWire")]
struct OrderWireWire {
    #[serde(rename = "a")]
    pub a: f64,
    #[serde(rename = "b")]
    pub b: bool,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "r")]
    pub r: bool,
    #[serde(rename = "t")]
    pub t: OrderType,
    #[serde(
        rename = "c",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub c: Option<String>,
}

impl Serialize for OrderWire {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderWireWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for OrderWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = OrderWire;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                OrderWireWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderRequest {
    pub coin: String,
    pub o: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelOrderRequest")]
struct CancelOrderRequestWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "o")]
    pub o: f64,
}

impl Serialize for CancelOrderRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelOrderRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelOrderRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelOrderRequest;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelOrderRequestWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderRequestsItem {
    pub a: f64,
    pub o: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelOrderRequestsItem")]
struct CancelOrderRequestsItemWire {
    #[serde(rename = "a")]
    pub a: f64,
    #[serde(rename = "o")]
    pub o: f64,
}

impl Serialize for CancelOrderRequestsItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelOrderRequestsItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelOrderRequestsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelOrderRequestsItem;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelOrderRequestsItemWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type CancelOrderRequests = Vec<CancelOrderRequestsItem>;

#[derive(Debug, Clone, PartialEq)]
pub struct CancelByCloidRequest {
    pub coin: String,
    pub cloid: Cloid,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelByCloidRequest")]
struct CancelByCloidRequestWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "cloid")]
    pub cloid: Cloid,
}

impl Serialize for CancelByCloidRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelByCloidRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelByCloidRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelByCloidRequest;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelByCloidRequestWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModifyRequest {
    pub oid: OidOrCloid,
    pub order: OrderRequest,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ModifyRequest")]
struct ModifyRequestWire {
    #[serde(rename = "oid")]
    pub oid: OidOrCloid,
    #[serde(rename = "order")]
    pub order: OrderRequest,
}

impl Serialize for ModifyRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ModifyRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ModifyRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ModifyRequest;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ModifyRequestWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModifyWire {
    pub oid: f64,
    pub order: OrderWire,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ModifyWire")]
struct ModifyWireWire {
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "order")]
    pub order: OrderWire,
}

impl Serialize for ModifyWire {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ModifyWireWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ModifyWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ModifyWire;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ModifyWireWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleCancelActionType {
    ScheduleCancel,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ScheduleCancelActionType")]
enum ScheduleCancelActionTypeWire {
    #[serde(rename = "scheduleCancel")]
    ScheduleCancel,
}

impl Serialize for ScheduleCancelActionType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ScheduleCancelActionTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ScheduleCancelActionType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ScheduleCancelActionTypeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleCancelAction {
    pub r#type: ScheduleCancelActionType,
    pub time: Option<f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ScheduleCancelAction")]
struct ScheduleCancelActionWire {
    #[serde(rename = "type")]
    pub r#type: ScheduleCancelActionType,
    #[serde(
        rename = "time",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub time: Option<f64>,
}

impl Serialize for ScheduleCancelAction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ScheduleCancelActionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ScheduleCancelAction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ScheduleCancelAction;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                ScheduleCancelActionWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub r: String,
    pub s: String,
    pub v: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Signature")]
struct SignatureWire {
    #[serde(rename = "r")]
    pub r: String,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "v")]
    pub v: f64,
}

impl Serialize for Signature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SignatureWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = Signature;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                SignatureWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub notification: String,
    pub user: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Notification")]
struct NotificationWire {
    #[serde(rename = "notification")]
    pub notification: String,
    #[serde(rename = "user")]
    pub user: String,
}

impl Serialize for Notification {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        NotificationWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Notification {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = Notification;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                NotificationWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candle {
    pub t: f64,
    pub t_upper: f64,
    pub s: String,
    pub i: String,
    pub o: String,
    pub c: String,
    pub h: String,
    pub l: String,
    pub v: String,
    pub n: f64,
    pub coin: String,
    pub interval: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Candle")]
struct CandleWire {
    #[serde(rename = "t")]
    pub t: f64,
    #[serde(rename = "T")]
    pub t_upper: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "h")]
    pub h: String,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "v")]
    pub v: String,
    #[serde(rename = "n")]
    pub n: f64,
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "interval")]
    pub interval: String,
}

impl Serialize for Candle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CandleWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Candle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = Candle;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CandleWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HyperliquidCandle {
    pub t: f64,
    pub t_upper: f64,
    pub s: String,
    pub i: String,
    pub o: String,
    pub c: String,
    pub h: String,
    pub l: String,
    pub v: String,
    pub n: f64,
    pub coin: String,
    pub interval: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "HyperliquidCandle")]
struct HyperliquidCandleWire {
    #[serde(rename = "t")]
    pub t: f64,
    #[serde(rename = "T")]
    pub t_upper: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "h")]
    pub h: String,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "v")]
    pub v: String,
    #[serde(rename = "n")]
    pub n: f64,
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "interval")]
    pub interval: String,
}

impl Serialize for HyperliquidCandle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        HyperliquidCandleWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for HyperliquidCandle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = HyperliquidCandle;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                HyperliquidCandleWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserFill {
    pub coin: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub time: f64,
    pub start_position: String,
    pub dir: String,
    pub closed_pnl: String,
    pub hash: String,
    pub oid: f64,
    pub crossed: bool,
    pub fee: String,
    pub tid: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserFill")]
struct WsUserFillWire {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "px")]
    pub px: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "startPosition")]
    pub start_position: String,
    #[serde(rename = "dir")]
    pub dir: String,
    #[serde(rename = "closedPnl")]
    pub closed_pnl: String,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "oid")]
    pub oid: f64,
    #[serde(rename = "crossed")]
    pub crossed: bool,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "tid")]
    pub tid: f64,
}

impl Serialize for WsUserFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserFillWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserFill {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserFill;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserFillWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserFills {
    pub is_snapshot: bool,
    pub fills: Vec<WsUserFill>,
    pub user: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserFills")]
struct WsUserFillsWire {
    #[serde(rename = "isSnapshot")]
    pub is_snapshot: bool,
    #[serde(rename = "fills")]
    pub fills: Vec<WsUserFill>,
    #[serde(rename = "user")]
    pub user: String,
}

impl Serialize for WsUserFills {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserFillsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserFills {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserFills;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserFillsWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserFundings {
    pub is_snapshot: bool,
    pub fundings: Vec<WsUserFunding>,
    pub user: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserFundings")]
struct WsUserFundingsWire {
    #[serde(rename = "isSnapshot")]
    pub is_snapshot: bool,
    #[serde(rename = "fundings")]
    pub fundings: Vec<WsUserFunding>,
    #[serde(rename = "user")]
    pub user: String,
}

impl Serialize for WsUserFundings {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserFundingsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserFundings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserFundings;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserFundingsWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WsUserNonFundingLedgerUpdateType {
    Deposit,
    Withdraw,
    Transfer,
    Liquidation,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserNonFundingLedgerUpdateType")]
enum WsUserNonFundingLedgerUpdateTypeWire {
    #[serde(rename = "deposit")]
    Deposit,
    #[serde(rename = "withdraw")]
    Withdraw,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "liquidation")]
    Liquidation,
}

impl Serialize for WsUserNonFundingLedgerUpdateType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserNonFundingLedgerUpdateTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserNonFundingLedgerUpdateType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WsUserNonFundingLedgerUpdateTypeWire::deserialize(serde::de::value::StrDeserializer::<
            D::Error,
        >::new(&text))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserNonFundingLedgerUpdate {
    pub time: f64,
    pub coin: String,
    pub usdc: String,
    pub r#type: WsUserNonFundingLedgerUpdateType,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserNonFundingLedgerUpdate")]
struct WsUserNonFundingLedgerUpdateWire {
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "usdc")]
    pub usdc: String,
    #[serde(rename = "type")]
    pub r#type: WsUserNonFundingLedgerUpdateType,
}

impl Serialize for WsUserNonFundingLedgerUpdate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserNonFundingLedgerUpdateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserNonFundingLedgerUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserNonFundingLedgerUpdate;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserNonFundingLedgerUpdateWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsUserNonFundingLedgerUpdates {
    pub is_snapshot: bool,
    pub updates: Vec<WsUserNonFundingLedgerUpdate>,
    pub user: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsUserNonFundingLedgerUpdates")]
struct WsUserNonFundingLedgerUpdatesWire {
    #[serde(rename = "isSnapshot")]
    pub is_snapshot: bool,
    #[serde(rename = "updates")]
    pub updates: Vec<WsUserNonFundingLedgerUpdate>,
    #[serde(rename = "user")]
    pub user: String,
}

impl Serialize for WsUserNonFundingLedgerUpdates {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WsUserNonFundingLedgerUpdatesWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WsUserNonFundingLedgerUpdates {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = WsUserNonFundingLedgerUpdates;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                WsUserNonFundingLedgerUpdatesWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerpDex {
    pub name: String,
    pub full_name: String,
    pub deployer: String,
    pub oracle_updater: Option<String>,
    pub fee_recipient: Option<String>,
    pub asset_to_streaming_oi_cap: Vec<(String, String)>,
    pub sub_deployers: Vec<(String, Vec<String>)>,
    pub deployer_fee_scale: String,
    pub last_deployer_fee_scale_change_time: String,
    pub asset_to_funding_multiplier: Vec<(String, String)>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PerpDex")]
struct PerpDexWire {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "fullName")]
    pub full_name: String,
    #[serde(rename = "deployer")]
    pub deployer: String,
    #[serde(
        rename = "oracleUpdater",
        deserialize_with = "deserialize_required_nullable"
    )]
    pub oracle_updater: Option<String>,
    #[serde(
        rename = "feeRecipient",
        deserialize_with = "deserialize_required_nullable"
    )]
    pub fee_recipient: Option<String>,
    #[serde(rename = "assetToStreamingOiCap")]
    pub asset_to_streaming_oi_cap: Vec<(String, String)>,
    #[serde(rename = "subDeployers")]
    pub sub_deployers: Vec<(String, Vec<String>)>,
    #[serde(rename = "deployerFeeScale")]
    pub deployer_fee_scale: String,
    #[serde(rename = "lastDeployerFeeScaleChangeTime")]
    pub last_deployer_fee_scale_change_time: String,
    #[serde(rename = "assetToFundingMultiplier")]
    pub asset_to_funding_multiplier: Vec<(String, String)>,
}

impl Serialize for PerpDex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PerpDexWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PerpDex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = PerpDex;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                PerpDexWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

pub type PerpDexs = Vec<Option<PerpDex>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Hip3MarketInfoMarginMode {
    StrictIsolated,
    NoCross,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Hip3MarketInfoMarginMode")]
enum Hip3MarketInfoMarginModeWire {
    #[serde(rename = "strictIsolated")]
    StrictIsolated,
    #[serde(rename = "noCross")]
    NoCross,
}

impl Serialize for Hip3MarketInfoMarginMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Hip3MarketInfoMarginModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Hip3MarketInfoMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Hip3MarketInfoMarginModeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hip3MarketInfoGrowthMode {
    Enabled,
    Disabled,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Hip3MarketInfoGrowthMode")]
enum Hip3MarketInfoGrowthModeWire {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

impl Serialize for Hip3MarketInfoGrowthMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Hip3MarketInfoGrowthModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Hip3MarketInfoGrowthMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Hip3MarketInfoGrowthModeWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&text),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hip3MarketInfo {
    pub name: String,
    pub sz_decimals: f64,
    pub max_leverage: f64,
    pub only_isolated: bool,
    pub margin_table_id: Option<f64>,
    pub margin_mode: Option<Hip3MarketInfoMarginMode>,
    pub growth_mode: Option<Hip3MarketInfoGrowthMode>,
    pub is_delisted: Option<bool>,
    pub last_growth_mode_change_time: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Hip3MarketInfo")]
struct Hip3MarketInfoWire {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "szDecimals")]
    pub sz_decimals: f64,
    #[serde(rename = "maxLeverage")]
    pub max_leverage: f64,
    #[serde(rename = "onlyIsolated")]
    pub only_isolated: bool,
    #[serde(
        rename = "marginTableId",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_table_id: Option<f64>,
    #[serde(
        rename = "marginMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_mode: Option<Hip3MarketInfoMarginMode>,
    #[serde(
        rename = "growthMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub growth_mode: Option<Hip3MarketInfoGrowthMode>,
    #[serde(
        rename = "isDelisted",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_delisted: Option<bool>,
    #[serde(
        rename = "lastGrowthModeChangeTime",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_growth_mode_change_time: Option<String>,
}

impl Serialize for Hip3MarketInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Hip3MarketInfoWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Hip3MarketInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = Hip3MarketInfo;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                Hip3MarketInfoWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuilderInfo {
    pub b: String,
    pub f: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BuilderInfo")]
struct BuilderInfoWire {
    #[serde(rename = "b")]
    pub b: String,
    #[serde(rename = "f")]
    pub f: f64,
}

impl Serialize for BuilderInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BuilderInfoWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BuilderInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = BuilderInfo;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                BuilderInfoWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuilderFeeApprovalResult {
    pub success: bool,
    pub message: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BuilderFeeApprovalResult")]
struct BuilderFeeApprovalResultWire {
    #[serde(rename = "success")]
    pub success: bool,
    #[serde(rename = "message")]
    pub message: String,
}

impl Serialize for BuilderFeeApprovalResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BuilderFeeApprovalResultWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BuilderFeeApprovalResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = BuilderFeeApprovalResult;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                BuilderFeeApprovalResultWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuilderFeeCheckResult {
    pub approved: bool,
    pub max_fee_rate: Option<String>,
    pub max_fee_rate_tenths_bps: Option<f64>,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BuilderFeeCheckResult")]
struct BuilderFeeCheckResultWire {
    #[serde(rename = "approved")]
    pub approved: bool,
    #[serde(
        rename = "maxFeeRate",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_fee_rate: Option<String>,
    #[serde(
        rename = "maxFeeRateTenthsBps",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_fee_rate_tenths_bps: Option<f64>,
    #[serde(
        rename = "error",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<String>,
}

impl Serialize for BuilderFeeCheckResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BuilderFeeCheckResultWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for BuilderFeeCheckResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = BuilderFeeCheckResult;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                BuilderFeeCheckResultWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for HyperliquidUserAbstractionMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { UnifiedAccount, PortfolioMargin, Disabled, Default, DexAbstraction, Standard });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Tif {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Alo, Ioc, Gtc });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Tpsl {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Tp, Sl });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LimitOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tif });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TriggerOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { trigger_px, is_market, tpsl });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Grouping {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Na, NormalTpsl, PositionTpsl });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { limit, trigger });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OidOrCloid {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Case0(value), Case1(value) });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for MetaUniverseItemMarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { StrictIsolated, NoCross });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for MetaUniverseItemGrowthMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Enabled, Disabled });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for MetaUniverseItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, sz_decimals, max_leverage, only_isolated, is_delisted, margin_table_id, margin_mode, growth_mode, last_growth_mode_change_time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Meta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { universe });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for ClearinghouseStateAssetPositionsItemPositionCumFunding
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { all_time, since_change, since_open });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for ClearinghouseStateAssetPositionsItemPositionLeverage
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { raw_usd, r#type, value });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for ClearinghouseStateAssetPositionsItemPosition
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, cum_funding, entry_px, leverage, liquidation_px, margin_used, max_leverage, position_value, return_on_equity, szi, unrealized_pnl });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ClearinghouseStateAssetPositionsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { position, r#type });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ClearinghouseStateCrossMarginSummary {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_value, total_margin_used, total_ntl_pos, total_raw_usd });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ClearinghouseStateMarginSummary {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_value, total_margin_used, total_ntl_pos, total_raw_usd });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ClearinghouseState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { asset_positions, cross_maintenance_margin_used, cross_margin_summary, margin_summary, time, withdrawable });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserFills {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, px, sz, side, time, start_position, dir, closed_pnl, hash, oid, crossed, fee, tid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase0RestingOid {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Case0(value), Case1(value) });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase0Resting {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { oid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase0 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { resting });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase1FilledOid {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Case0(value), Case1(value) });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase1Filled {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { oid, total_sz, avg_px });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase1 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { filled });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase2 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { error });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase3 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { WaitingForFill });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatusCase4 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { WaitingForTrigger });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Case0(value), Case1(value), Case2(value), Case3(value), Case4(value) });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseResponseCase1Data {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { statuses });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseResponseCase1 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type, data });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Case0(value), Case1(value) });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { status, response });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsTrade {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, side, px, sz, hash, time, tid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsBook {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, levels, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsBbo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, bbo, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { px, sz, n });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsOrderOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, side, limit_px, sz, oid, timestamp, orig_sz });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order, status, status_timestamp, user });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsFill {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, px, sz, side, time, start_position, dir, closed_pnl, hash, oid, crossed, fee, tid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserFunding {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { time, coin, usdc, szi, funding_rate });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsLiquidation {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { lid, liquidator, liquidated_user, liquidated_ntl_pos, liquidated_account_value });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsNonUserCancel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, oid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotClearinghouseBalance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, token, hold, total, entry_ntl });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotClearinghouseState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { balances, token_to_available_after_maintenance });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for FrontendOpenOrders {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, is_position_tpsl, is_trigger, limit_px, oid, order_type, orig_sz, reduce_only, side, sz, timestamp, trigger_condition, trigger_px, tif });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for L2BookLevelsItem0Item {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { px, sz, n });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for L2BookLevelsItem1Item {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { px, sz, n });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for L2Book {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { levels });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CandleSnapshot {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { t_upper, c, h, i, l, n, o, s, t, v });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AssetCtx {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { day_ntl_vlm, day_base_vlm, funding, impact_pxs, mark_px, mid_px, open_interest, oracle_px, premium, prev_day_px });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for MetaAndAssetCtxs {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { meta, asset_ctxs });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserFundingDeltaType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Funding });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserFundingDelta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, funding_rate, szi, r#type, usdc });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserFundingEntry {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { delta, hash, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserNonFundingLedgerDeltaType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Deposit, Withdraw, Transfer, Liquidation });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserNonFundingLedgerDelta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, r#type, usdc });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserNonFundingLedgerEntry {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { delta, hash, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for FundingHistoryEntry {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, funding_rate, premium, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotTokenEvmContract {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { address, evm_extra_wei_decimals });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotToken {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, sz_decimals, wei_decimals, index, token_id, is_canonical, full_name, evm_contract, deployer_trading_fee_share });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotMarket {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, tokens, index, is_canonical });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotMeta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tokens, universe });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotAssetCtx {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { day_ntl_vlm, mark_px, mid_px, prev_day_px, coin, circulating_supply, total_supply, day_base_vlm });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for SpotMetaAndAssetCtxs {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { meta, asset_ctxs });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserOpenOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, limit_px, oid, side, sz, timestamp });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, is_buy, sz, limit_px, order_type, reduce_only, cloid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderWire {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { a, b, p, s, r, t, c });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CancelOrderRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, o });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CancelOrderRequestsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { a, o });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CancelByCloidRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, cloid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ModifyRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { oid, order });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ModifyWire {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { oid, order });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ScheduleCancelActionType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { ScheduleCancel });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ScheduleCancelAction {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Signature {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r, s, v });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Notification {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { notification, user });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Candle {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { t, t_upper, s, i, o, c, h, l, v, n, coin, interval });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for HyperliquidCandle {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { t, t_upper, s, i, o, c, h, l, v, n, coin, interval });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserFill {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, px, sz, side, time, start_position, dir, closed_pnl, hash, oid, crossed, fee, tid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserFills {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { is_snapshot, fills, user });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserFundings {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { is_snapshot, fundings, user });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserNonFundingLedgerUpdateType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Deposit, Withdraw, Transfer, Liquidation });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserNonFundingLedgerUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { time, coin, usdc, r#type });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WsUserNonFundingLedgerUpdates {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { is_snapshot, updates, user });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for PerpDex {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, full_name, deployer, oracle_updater, fee_recipient, asset_to_streaming_oi_cap, sub_deployers, deployer_fee_scale, last_deployer_fee_scale_change_time, asset_to_funding_multiplier });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Hip3MarketInfoMarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { StrictIsolated, NoCross });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Hip3MarketInfoGrowthMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Enabled, Disabled });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for Hip3MarketInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, sz_decimals, max_leverage, only_isolated, margin_table_id, margin_mode, growth_mode, is_delisted, last_growth_mode_change_time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BuilderInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { b, f });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BuilderFeeApprovalResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, message });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BuilderFeeCheckResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { approved, max_fee_rate, max_fee_rate_tenths_bps, error });
    }
}
