//! Spot declarations from hyperliquidspot.types.ts.
//! Keep spot URL/query tables separate: the perpetual exports differ.

pub const BASE_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "https://api.hyperliquid.xyz"),
    ("TESTNET", "https://api.hyperliquid-testnet.xyz"),
];

pub const WSS_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "wss://api.hyperliquid.xyz/ws"),
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
    ("MAINNET_URL", "wss://api.hyperliquid.xyz/ws"),
    ("TESTNET_URL", "wss://api.hyperliquid-testnet.xyz/ws"),
];

// These six closed declarations match the perpetual module structurally,
// including all transitive references. spot-types.cjs checks the actual ASTs.
// Reuse their strict map/string ingress and binary64 number handling.
pub type Tif = crate::types::Tif;
pub type Tpsl = crate::types::Tpsl;
pub type LimitOrderType = crate::types::LimitOrderType;
pub type TriggerOrderType = crate::types::TriggerOrderType;
pub type Grouping = crate::types::Grouping;
pub type OrderType = crate::types::OrderType;

// Numeric cancellation lists have the same closed source declaration.
pub type CancelOrderRequests = crate::types::CancelOrderRequests;

/// Spot explicitly permits null; the perpetual declaration does not.
/// None is absent, Some(None) is null, Some(Some(value)) is a number.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleCancelAction {
    pub r#type: crate::types::ScheduleCancelActionType,
    pub time: Option<Option<f64>>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "ScheduleCancelAction")]
struct ScheduleCancelActionWire {
    #[serde(rename = "type")]
    pub r#type: crate::types::ScheduleCancelActionType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub time: Option<Option<f64>>,
}
fn present_nullable<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<Option<f64>>, D::Error> {
    <Option<f64> as serde::Deserialize>::deserialize(decoder).map(Some)
}
impl serde::Serialize for ScheduleCancelAction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ScheduleCancelActionWire::serialize(self, serializer)
    }
}
impl<'de> serde::Deserialize<'de> for ScheduleCancelAction {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = ScheduleCancelAction;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object record")
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
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                decoder,
            )?;
        input
            .decode_with(|input| serde::Deserializer::deserialize_map(input, OnlyMap))
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ScheduleCancelAction {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type, time });
    }
}
