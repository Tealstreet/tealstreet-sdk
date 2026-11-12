//! Shared store models used by the Store contract; source: packages/safe-cex/src/types.ts.
use crate::{
    Balance, MarginType, Market, Order, OrderRiskProfile, OrderSide, Position, PositionMode,
    PositionSide, RequestOutcomeCounts, Ticker, deserialize_present,
};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskOrigin {
    Web,
    Cli,
    Webhook,
    Automation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TwapStatus {
    Idle,
    Running,
    Paused,
    Completed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SliceAction {
    Market,
    Next,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayedPosition {
    /// TS adds key via a local intersection cast on real positions and OMITS it
    /// on fake positions. This Option represents the observable heterogeneous array.
    pub key: Option<String>,
    pub position: Position,
    pub account: String,
    pub ticker: Option<Ticker>,
    pub market: Option<Market>,
    pub balance: Option<Balance>,
    pub exchange_name: Option<String>,
    pub one_min_upnl: Option<f64>,
    pub five_min_upnl: Option<f64>,
    pub has_active_chaser: Option<bool>,
    pub has_active_twap: Option<bool>,
    pub is_fake_position: Option<bool>,
    pub chaser_size: Option<f64>,
    pub has_full_stop: Option<bool>,
    pub has_full_tp: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveChaser {
    pub id: String,
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    #[serde(rename = "maxAttempts")]
    pub max_attempts: f64,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "reduceOnly")]
    pub reduce_only: Option<bool>,
    pub attempts: f64,
    #[serde(rename = "isStopped")]
    pub is_stopped: bool,
    #[serde(rename = "stopReason")]
    pub stop_reason: String,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "currentOrderIds")]
    pub current_order_ids: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin: Option<TaskOrigin>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "twapId")]
    pub twap_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTWAP {
    pub id: String,
    pub symbol: String,
    #[serde(rename = "accountName")]
    pub account_name: String,
    #[serde(rename = "activeMarket")]
    pub active_market: Market,
    pub size: f64,
    pub duration: f64,
    #[serde(rename = "ordersCount")]
    pub orders_count: f64,
    pub side: OrderSide,
    #[serde(rename = "chaserDelay")]
    pub chaser_delay: f64,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: bool,
    #[serde(rename = "randomnessPercentage")]
    pub randomness_percentage: f64,
    #[serde(rename = "useLimitOrders")]
    pub use_limit_orders: bool,
    #[serde(rename = "useChaserOrders")]
    pub use_chaser_orders: bool,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "timeoutPct")]
    pub timeout_pct: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "timeoutAction")]
    pub timeout_action: Option<SliceAction>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "failAction")]
    pub fail_action: Option<SliceAction>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin: Option<TaskOrigin>,
    #[serde(rename = "ordersSizes")]
    pub orders_sizes: Vec<f64>,
    pub status: TwapStatus,
    #[serde(rename = "ordersExecuted")]
    pub orders_executed: f64,
    #[serde(rename = "nextOrderAt")]
    pub next_order_at: f64,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "markForCleanup")]
    pub mark_for_cleanup: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "createdAt")]
    pub created_at: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "startedAt")]
    pub started_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct StoreDataLoaded {
    pub balance: bool,
    pub orders: bool,
    pub markets: bool,
    pub tickers: bool,
    pub positions: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct StoreOptions {
    pub is_hedged: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct FetchedLeverage {
    /// @ts-mutable
    pub buy_leverage: f64,
    /// @ts-mutable
    pub sell_leverage: f64,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_type: Option<MarginType>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_mode: Option<PositionMode>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_fetching: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct DisplayedMargin {
    #[serde(deserialize_with = "Option::<f64>::deserialize")]
    #[ts(type = "number | null")]
    pub long_exposure: Option<f64>,
    #[serde(deserialize_with = "Option::<f64>::deserialize")]
    #[ts(type = "number | null")]
    pub short_exposure: Option<f64>,
    #[serde(deserialize_with = "Option::<f64>::deserialize")]
    #[ts(type = "number | null")]
    pub total_notional: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct StoreUpdateIds {
    /// @ts-mutable
    #[serde(rename = "positions")]
    pub positions: f64,
    /// @ts-mutable
    #[serde(rename = "orders")]
    pub orders: f64,
}

/// Last reported per-position settings, separate from current exposure.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PositionSettings {
    pub id: String,
    pub symbol: String,
    pub side: PositionSide,
    pub leverage: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_type: Option<MarginType>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_mode: Option<PositionMode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct StoreData {
    #[serde(deserialize_with = "Option::<f64>::deserialize")]
    #[ts(type = "number | null")]
    pub latency: Option<f64>,
    pub balance: Balance,
    pub markets: Vec<Market>,
    pub tickers: Vec<Ticker>,
    pub orders: Vec<Order>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "starkConfig")]
    #[ts(
        type = "{ usdtConfig?: unknown; usdcConfig?: unknown; positionId?: unknown; takerFeePerToken?: unknown }",
        optional
    )]
    pub stark_config: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "zkLinkConfig")]
    #[ts(
        type = "{ contractConfig?: unknown; l2Key?: unknown; ethereumAddress?: unknown; id?: unknown; spotAccount?: unknown; spotWallets?: unknown; contractAccount?: unknown; contractWallets?: unknown }",
        optional
    )]
    pub zk_link_config: Option<crate::retained_json::RetainedJsonInput>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "userData")]
    #[ts(type = "unknown", optional)]
    pub user_data: Option<Value>,
    pub positions: Vec<Position>,
    #[serde(default)]
    #[ts(type = "Record<string, PositionSettings>")]
    pub position_settings: IndexMap<String, PositionSettings>,
    #[serde(rename = "displayedPositions")]
    #[ts(type = "DisplayedPosition[]")]
    pub displayed_positions: Vec<DisplayedPosition>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "displayedMargin")]
    pub displayed_margin: Option<DisplayedMargin>,
    #[serde(rename = "ordersRisk")]
    #[ts(type = "Record<string, OrderRiskProfile[]> ")]
    pub orders_risk: IndexMap<String, Vec<OrderRiskProfile>>,
    #[serde(rename = "activeChasers")]
    #[ts(type = "ActiveChaser[]")]
    pub active_chasers: Vec<ActiveChaser>,
    #[serde(rename = "activeTWAPs")]
    #[ts(type = "ActiveTWAP[]")]
    pub active_twaps: Vec<ActiveTWAP>,
    pub loaded: StoreDataLoaded,
    pub options: StoreOptions,
    #[serde(rename = "updateIds")]
    pub update_ids: StoreUpdateIds,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "messageHandlerStats")]
    #[ts(
        type = "Partial<Record<MessageHandlerStatsType, MessageHandlersStats>>",
        optional
    )]
    pub message_handler_stats: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "requestStats")]
    pub request_stats: Option<RequestOutcomeCounts>,
    #[serde(rename = "fetchedLeverage")]
    #[ts(type = "Record<string, FetchedLeverage> ")]
    pub fetched_leverage: IndexMap<String, FetchedLeverage>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "socketHealth")]
    #[ts(
        type = "{ publicHealthy: boolean; privateHealthy: boolean; publicInitialized: boolean; privateInitialized: boolean; publicLastMessage?: number; privateLastMessage?: number; reconnectDiagnostics?: AccountReconnectDiagnostics; publicCloseCode?: number; privateCloseCode?: number; publicCloseCountTotal?: number; privateCloseCountTotal?: number; publicCloseStreak?: number; privateCloseStreak?: number; publicReconnectPaused?: boolean; privateReconnectPaused?: boolean; privateDegradedReason?: string; }",
        optional
    )]
    pub socket_health: Option<crate::retained_json::RetainedJsonInput>,
}

/// The initial host Store. Native account frames replace its unloaded exchange state.
pub fn initial_store_data() -> StoreData {
    StoreData {
        latency: None,
        balance: Balance {
            used: 0.0,
            free: 0.0,
            total: 0.0,
            upnl: 0.0,
            info: None,
            currencies: None,
            coins: None,
        },
        markets: vec![],
        tickers: vec![],
        orders: vec![],
        positions: vec![],
        position_settings: IndexMap::new(),
        displayed_positions: vec![],
        stark_config: None,
        zk_link_config: None,
        user_data: None,
        displayed_margin: Some(DisplayedMargin {
            long_exposure: Some(0.0),
            short_exposure: Some(0.0),
            total_notional: Some(0.0),
        }),
        orders_risk: IndexMap::new(),
        active_chasers: vec![],
        active_twaps: vec![],
        loaded: StoreDataLoaded {
            balance: false,
            orders: false,
            markets: false,
            tickers: false,
            positions: false,
        },
        options: StoreOptions { is_hedged: false },
        update_ids: StoreUpdateIds {
            positions: 0.0,
            orders: 0.0,
        },
        message_handler_stats: Some(serde_json::json!({})),
        request_stats: Some(RequestOutcomeCounts {
            ok: 0.0,
            rate_limit: 0.0,
            auth: 0.0,
            network: 0.0,
            other: 0.0,
        }),
        fetched_leverage: IndexMap::new(),
        socket_health: Some(crate::retained_json::RetainedJsonInput::from(
            serde_json::json!({
                "publicHealthy": false, "privateHealthy": false, "publicInitialized": false, "privateInitialized": false
            }),
        )),
    }
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionSettings")]
    #[serde(rename_all = "camelCase")]
    struct WirePositionSettings {
        id: String,
        symbol: String,
        side: PositionSide,
        leverage: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        margin_type: Option<MarginType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_mode: Option<PositionMode>,
    }
    impl<'de> serde::Deserialize<'de> for super::PositionSettings {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionSettings::deserialize(crate::strict_decode::object(deserializer))
        }
    }
    use super::{MarginType, PositionMode, PositionSide, deserialize_present};
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreDataLoaded")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreDataLoaded {
        balance: bool,
        orders: bool,
        markets: bool,
        tickers: bool,
        positions: bool,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreDataLoaded {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreDataLoaded::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreOptions {
        is_hedged: bool,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::FetchedLeverage")]
    #[serde(rename_all = "camelCase")]
    struct WireFetchedLeverage {
        buy_leverage: f64,
        sell_leverage: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        margin_type: Option<MarginType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_mode: Option<PositionMode>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_fetching: Option<bool>,
    }

    impl<'de> serde::Deserialize<'de> for super::FetchedLeverage {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireFetchedLeverage::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::DisplayedMargin")]
    #[serde(rename_all = "camelCase")]
    struct WireDisplayedMargin {
        #[serde(deserialize_with = "Option::<f64>::deserialize")]
        long_exposure: Option<f64>,
        #[serde(deserialize_with = "Option::<f64>::deserialize")]
        short_exposure: Option<f64>,
        #[serde(deserialize_with = "Option::<f64>::deserialize")]
        total_notional: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::DisplayedMargin {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireDisplayedMargin::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreUpdateIds")]
    struct WireStoreUpdateIds {
        #[serde(rename = "positions")]
        positions: f64,
        #[serde(rename = "orders")]
        orders: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreUpdateIds {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreUpdateIds::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[cfg(test)]
    use super::StoreData;
    use super::{
        ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage,
        IndexMap, Market, Order, OrderRiskProfile, OrderSide, Position, PositionSettings,
        RequestOutcomeCounts, SliceAction, StoreDataLoaded, StoreOptions, StoreUpdateIds,
        TaskOrigin, Ticker, TwapStatus, Value,
    };
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::TaskOrigin")]
    #[serde(rename_all = "lowercase")]
    enum WireTaskOrigin {
        Web,
        Cli,
        Webhook,
        Automation,
    }

    impl<'de> serde::Deserialize<'de> for super::TaskOrigin {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireTaskOrigin::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::TwapStatus")]
    #[serde(rename_all = "lowercase")]
    enum WireTwapStatus {
        Idle,
        Running,
        Paused,
        Completed,
    }

    impl<'de> serde::Deserialize<'de> for super::TwapStatus {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireTwapStatus::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::SliceAction")]
    #[serde(rename_all = "lowercase")]
    enum WireSliceAction {
        Market,
        Next,
    }

    impl<'de> serde::Deserialize<'de> for super::SliceAction {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireSliceAction::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    // The remote serializer pins the same flattened schema as the public type.
    // Its decoder below streams the map instead of deriving Content buffering.
    #[derive(serde::Serialize)]
    #[serde(remote = "super::DisplayedPosition")]
    #[serde(rename_all = "camelCase")]
    struct WireDisplayedPosition {
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        key: Option<String>,
        #[serde(flatten)]
        position: Position,
        account: String,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        ticker: Option<Ticker>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        market: Option<Market>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        balance: Option<Balance>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "exchangeName")]
        exchange_name: Option<String>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "oneMinUpnl")]
        one_min_upnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "fiveMinUpnl")]
        five_min_upnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "hasActiveChaser")]
        has_active_chaser: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "hasActiveTWAP")]
        has_active_twap: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "isFakePosition")]
        is_fake_position: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "chaserSize")]
        chaser_size: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "hasFullStop")]
        has_full_stop: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "hasFullTP")]
        has_full_tp: Option<bool>,
    }

    // Flattening through Serde's Content buffer turns arbitrary-precision JSON
    // decimals into map events. Filter display-only fields on the original map
    // instead: Position and its retained info decoder receive unchanged values.
    struct DisplayedPositionMap<A> {
        map: A,
        key: Option<String>,
        account: Option<String>,
        ticker: Option<Ticker>,
        market: Option<Market>,
        balance: Option<Balance>,
        exchange_name: Option<String>,
        one_min_upnl: Option<f64>,
        five_min_upnl: Option<f64>,
        has_active_chaser: Option<bool>,
        has_active_twap: Option<bool>,
        is_fake_position: Option<bool>,
        chaser_size: Option<f64>,
        has_full_stop: Option<bool>,
        has_full_tp: Option<bool>,
    }

    impl<'de, A: serde::de::MapAccess<'de>> serde::de::MapAccess<'de> for DisplayedPositionMap<A> {
        type Error = A::Error;

        fn next_key_seed<K: serde::de::DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, Self::Error> {
            while let Some(key) = self.map.next_key::<String>()? {
                match key.as_str() {
                    "key" => {
                        if self.key.is_some() {
                            return Err(serde::de::Error::duplicate_field("key"));
                        }
                        self.key = Some(self.map.next_value()?);
                    }
                    "account" => {
                        if self.account.is_some() {
                            return Err(serde::de::Error::duplicate_field("account"));
                        }
                        self.account = Some(self.map.next_value()?);
                    }
                    "ticker" => {
                        if self.ticker.is_some() {
                            return Err(serde::de::Error::duplicate_field("ticker"));
                        }
                        self.ticker = Some(self.map.next_value()?);
                    }
                    "market" => {
                        if self.market.is_some() {
                            return Err(serde::de::Error::duplicate_field("market"));
                        }
                        self.market = Some(self.map.next_value()?);
                    }
                    "balance" => {
                        if self.balance.is_some() {
                            return Err(serde::de::Error::duplicate_field("balance"));
                        }
                        self.balance = Some(self.map.next_value()?);
                    }
                    "exchangeName" => {
                        if self.exchange_name.is_some() {
                            return Err(serde::de::Error::duplicate_field("exchangeName"));
                        }
                        self.exchange_name = Some(self.map.next_value()?);
                    }
                    "oneMinUpnl" => {
                        if self.one_min_upnl.is_some() {
                            return Err(serde::de::Error::duplicate_field("oneMinUpnl"));
                        }
                        self.one_min_upnl = Some(self.map.next_value()?);
                    }
                    "fiveMinUpnl" => {
                        if self.five_min_upnl.is_some() {
                            return Err(serde::de::Error::duplicate_field("fiveMinUpnl"));
                        }
                        self.five_min_upnl = Some(self.map.next_value()?);
                    }
                    "hasActiveChaser" => {
                        if self.has_active_chaser.is_some() {
                            return Err(serde::de::Error::duplicate_field("hasActiveChaser"));
                        }
                        self.has_active_chaser = Some(self.map.next_value()?);
                    }
                    "hasActiveTWAP" => {
                        if self.has_active_twap.is_some() {
                            return Err(serde::de::Error::duplicate_field("hasActiveTWAP"));
                        }
                        self.has_active_twap = Some(self.map.next_value()?);
                    }
                    "isFakePosition" => {
                        if self.is_fake_position.is_some() {
                            return Err(serde::de::Error::duplicate_field("isFakePosition"));
                        }
                        self.is_fake_position = Some(self.map.next_value()?);
                    }
                    "chaserSize" => {
                        if self.chaser_size.is_some() {
                            return Err(serde::de::Error::duplicate_field("chaserSize"));
                        }
                        self.chaser_size = Some(self.map.next_value()?);
                    }
                    "hasFullStop" => {
                        if self.has_full_stop.is_some() {
                            return Err(serde::de::Error::duplicate_field("hasFullStop"));
                        }
                        self.has_full_stop = Some(self.map.next_value()?);
                    }
                    "hasFullTP" => {
                        if self.has_full_tp.is_some() {
                            return Err(serde::de::Error::duplicate_field("hasFullTP"));
                        }
                        self.has_full_tp = Some(self.map.next_value()?);
                    }
                    _ => {
                        return seed
                            .deserialize(serde::de::value::StringDeserializer::new(key))
                            .map(Some);
                    }
                }
            }
            Ok(None)
        }

        fn next_value_seed<V: serde::de::DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, Self::Error> {
            self.map.next_value_seed(seed)
        }
    }

    struct DisplayedPositionVisitor;
    impl<'de> serde::de::Visitor<'de> for DisplayedPositionVisitor {
        type Value = WireDisplayedPosition;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a displayed position map")
        }

        fn visit_newtype_struct<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_map(self)
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            use serde::de::MapAccess;
            let mut fields = DisplayedPositionMap {
                map,
                key: None,
                account: None,
                ticker: None,
                market: None,
                balance: None,
                exchange_name: None,
                one_min_upnl: None,
                five_min_upnl: None,
                has_active_chaser: None,
                has_active_twap: None,
                is_fake_position: None,
                chaser_size: None,
                has_full_stop: None,
                has_full_tp: None,
            };
            let position = <Position as serde::Deserialize>::deserialize(
                serde::de::value::MapAccessDeserializer::new(&mut fields),
            )?;
            // Consume any remaining entries even if the inner decoder
            // finishes early, so trailing display fields are observed.
            while fields.next_key::<serde::de::IgnoredAny>()?.is_some() {
                fields.next_value::<serde::de::IgnoredAny>()?;
            }
            Ok(WireDisplayedPosition {
                position,
                key: fields.key,
                account: fields
                    .account
                    .ok_or_else(|| serde::de::Error::missing_field("account"))?,
                ticker: fields.ticker,
                market: fields.market,
                balance: fields.balance,
                exchange_name: fields.exchange_name,
                one_min_upnl: fields.one_min_upnl,
                five_min_upnl: fields.five_min_upnl,
                has_active_chaser: fields.has_active_chaser,
                has_active_twap: fields.has_active_twap,
                is_fake_position: fields.is_fake_position,
                chaser_size: fields.chaser_size,
                has_full_stop: fields.has_full_stop,
                has_full_tp: fields.has_full_tp,
            })
        }
    }

    impl WireDisplayedPosition {
        fn deserialize<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Self, D::Error> {
            deserializer.deserialize_newtype_struct(
                crate::model_field::INFO_RECORD,
                DisplayedPositionVisitor,
            )
        }
    }

    impl<'de> serde::Deserialize<'de> for super::DisplayedPosition {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let wire = WireDisplayedPosition::deserialize(deserializer)?;
            Ok(Self {
                key: wire.key,
                position: wire.position,
                account: wire.account,
                ticker: wire.ticker,
                market: wire.market,
                balance: wire.balance,
                exchange_name: wire.exchange_name,
                one_min_upnl: wire.one_min_upnl,
                five_min_upnl: wire.five_min_upnl,
                has_active_chaser: wire.has_active_chaser,
                has_active_twap: wire.has_active_twap,
                is_fake_position: wire.is_fake_position,
                chaser_size: wire.chaser_size,
                has_full_stop: wire.has_full_stop,
                has_full_tp: wire.has_full_tp,
            })
        }
    }

    #[cfg(test)]
    #[test]
    fn displayed_position_companion_streams_decimal_and_signed_zero() {
        let text = r#"{"id":"wire","symbol":"WIRE","side":"short",
            "entryPrice":1.25,"notional":2.5,"leverage":3.75,"unrealizedPnl":-0,
            "contracts":4.125,"liquidationPrice":5.5,"oneMinUpnl":-0,
            "account":"wire-account"}"#;
        let mut input = serde_json::Deserializer::from_str(text);
        let wire = WireDisplayedPosition::deserialize(&mut input).unwrap();
        input.end().unwrap();
        assert_eq!(wire.position.id, "wire");
        assert_eq!(wire.account, "wire-account");
        assert_eq!(wire.position.entry_price, 1.25);
        assert_eq!(wire.position.notional, Some(2.5));
        assert_eq!(wire.position.leverage, 3.75);
        assert_eq!(wire.position.contracts, 4.125);
        assert_eq!(wire.position.liquidation_price, Some(5.5));
        assert_eq!(wire.position.unrealized_pnl.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(wire.one_min_upnl.unwrap().to_bits(), (-0.0_f64).to_bits());
        let mut wrong = serde_json::Deserializer::from_str("[]");
        assert!(WireDisplayedPosition::deserialize(&mut wrong).is_err());
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ActiveChaser")]
    #[serde(rename_all = "camelCase")]
    struct WireActiveChaser {
        id: String,
        symbol: String,
        size: f64,
        side: OrderSide,
        #[serde(rename = "maxAttempts")]
        max_attempts: f64,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "reduceOnly")]
        reduce_only: Option<bool>,
        attempts: f64,
        #[serde(rename = "isStopped")]
        is_stopped: bool,
        #[serde(rename = "stopReason")]
        stop_reason: String,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "currentOrderIds")]
        current_order_ids: Option<Vec<String>>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        origin: Option<TaskOrigin>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "twapId")]
        twap_id: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::ActiveChaser {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireActiveChaser::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ActiveTWAP")]
    #[serde(rename_all = "camelCase")]
    struct WireActiveTWAP {
        id: String,
        symbol: String,
        #[serde(rename = "accountName")]
        account_name: String,
        #[serde(rename = "activeMarket")]
        active_market: Market,
        size: f64,
        duration: f64,
        #[serde(rename = "ordersCount")]
        orders_count: f64,
        side: OrderSide,
        #[serde(rename = "chaserDelay")]
        chaser_delay: f64,
        #[serde(rename = "reduceOnly")]
        reduce_only: bool,
        #[serde(rename = "randomnessPercentage")]
        randomness_percentage: f64,
        #[serde(rename = "useLimitOrders")]
        use_limit_orders: bool,
        #[serde(rename = "useChaserOrders")]
        use_chaser_orders: bool,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "timeoutPct")]
        timeout_pct: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "timeoutAction")]
        timeout_action: Option<SliceAction>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "failAction")]
        fail_action: Option<SliceAction>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        origin: Option<TaskOrigin>,
        #[serde(rename = "ordersSizes")]
        orders_sizes: Vec<f64>,
        status: TwapStatus,
        #[serde(rename = "ordersExecuted")]
        orders_executed: f64,
        #[serde(rename = "nextOrderAt")]
        next_order_at: f64,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "markForCleanup")]
        mark_for_cleanup: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "createdAt")]
        created_at: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "startedAt")]
        started_at: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::ActiveTWAP {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireActiveTWAP::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreData")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreData {
        #[serde(deserialize_with = "Option::<f64>::deserialize")]
        latency: Option<f64>,
        balance: Balance,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        orders: Vec<Order>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "starkConfig")]
        stark_config: Option<Value>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "zkLinkConfig")]
        zk_link_config: Option<crate::retained_json::RetainedJsonInput>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "userData")]
        user_data: Option<Value>,
        positions: Vec<Position>,
        #[serde(default)]
        position_settings: IndexMap<String, PositionSettings>,
        #[serde(rename = "displayedPositions")]
        displayed_positions: Vec<DisplayedPosition>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "displayedMargin")]
        displayed_margin: Option<DisplayedMargin>,
        #[serde(rename = "ordersRisk")]
        orders_risk: IndexMap<String, Vec<OrderRiskProfile>>,
        #[serde(rename = "activeChasers")]
        active_chasers: Vec<ActiveChaser>,
        #[serde(rename = "activeTWAPs")]
        active_twaps: Vec<ActiveTWAP>,
        loaded: StoreDataLoaded,
        options: StoreOptions,
        #[serde(rename = "updateIds")]
        update_ids: StoreUpdateIds,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "messageHandlerStats")]
        message_handler_stats: Option<Value>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "requestStats")]
        request_stats: Option<RequestOutcomeCounts>,
        #[serde(rename = "fetchedLeverage")]
        fetched_leverage: IndexMap<String, FetchedLeverage>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "socketHealth")]
        socket_health: Option<crate::retained_json::RetainedJsonInput>,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreData {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct StoreRecordVisitor;
            impl<'de> serde::de::Visitor<'de> for StoreRecordVisitor {
                type Value = super::StoreData;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a Store data record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    deserializer: D,
                ) -> Result<Self::Value, D::Error> {
                    WireStoreData::deserialize(crate::strict_decode::object(deserializer))
                }
            }
            deserializer.deserialize_newtype_struct(crate::STORE_RECORD, StoreRecordVisitor)
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;
        #[test]
        fn every_shared_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::string_union::<TaskOrigin>(
                "TaskOrigin",
                WireTaskOrigin::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(TaskOrigin::Web)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TaskOrigin::Cli)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TaskOrigin::Webhook)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TaskOrigin::Automation)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<TwapStatus>(
                "TwapStatus",
                WireTwapStatus::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(TwapStatus::Idle)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TwapStatus::Running)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TwapStatus::Paused)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TwapStatus::Completed)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<SliceAction>(
                "SliceAction",
                WireSliceAction::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(SliceAction::Market)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(SliceAction::Next)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<DisplayedPosition>("DisplayedPosition");
            crate::strict_decode::tests::record::<ActiveChaser>("ActiveChaser");
            crate::strict_decode::tests::record::<ActiveTWAP>("ActiveTWAP");
            crate::strict_decode::tests::record::<StoreData>("StoreData");
            crate::strict_decode::tests::record::<StoreDataLoaded>("StoreDataLoaded");
            crate::strict_decode::tests::record::<StoreOptions>("StoreOptions");
            crate::strict_decode::tests::record::<FetchedLeverage>("FetchedLeverage");
            crate::strict_decode::tests::record::<super::super::PositionSettings>(
                "PositionSettings",
            );
            crate::strict_decode::tests::record::<DisplayedMargin>("DisplayedMargin");
            crate::strict_decode::tests::record::<StoreUpdateIds>("StoreUpdateIds");
        }
    }
}
