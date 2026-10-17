//! Typed wire models from hyperliquid.types.ts; JS numbers remain binary64.
use serde::{Deserialize, Serialize};

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
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

pub type AllMids = crate::json_models::AllMids;

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

impl safe_cex_num::numeric_ingress::JsNumberOutput for FundingHistoryEntry {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, funding_rate, premium, time });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for UserOpenOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { coin, limit_px, oid, side, sz, timestamp });
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
