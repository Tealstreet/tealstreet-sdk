//! Shared store models used by the Store contract; source: packages/safe-cex/src/types.ts.
use crate::{
    Balance, DisplayedMargin, FetchedLeverage, Market, Order, OrderRiskProfile, OrderSide,
    Position, RequestOutcomeCounts, StoreDataLoaded, StoreOptions, StoreUpdateIds, Ticker,
};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;

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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayedPosition {
    /// TS adds key via a local intersection cast on real positions and OMITS it
    /// on fake positions. This Option represents the observable heterogeneous array.
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub key: Option<String>,
    #[serde(flatten)]
    pub position: Position,
    pub account: String,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub ticker: Option<Ticker>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub market: Option<Market>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub balance: Option<Balance>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "exchangeName")]
    pub exchange_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "oneMinUpnl")]
    pub one_min_upnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "fiveMinUpnl")]
    pub five_min_upnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "hasActiveChaser")]
    pub has_active_chaser: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "hasActiveTWAP")]
    pub has_active_twap: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "isFakePosition")]
    pub is_fake_position: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "chaserSize")]
    pub chaser_size: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "hasFullStop")]
    pub has_full_stop: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "hasFullTP")]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreData {
    pub latency: f64,
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
    pub stark_config: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "zkLinkConfig")]
    pub zk_link_config: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "userData")]
    pub user_data: Option<Value>,
    pub positions: Vec<Position>,
    #[serde(rename = "displayedPositions")]
    pub displayed_positions: Vec<DisplayedPosition>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "displayedMargin")]
    pub displayed_margin: Option<DisplayedMargin>,
    #[serde(rename = "ordersRisk")]
    pub orders_risk: IndexMap<String, Vec<OrderRiskProfile>>,
    #[serde(rename = "activeChasers")]
    pub active_chasers: Vec<ActiveChaser>,
    #[serde(rename = "activeTWAPs")]
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
    pub message_handler_stats: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "requestStats")]
    pub request_stats: Option<RequestOutcomeCounts>,
    #[serde(rename = "fetchedLeverage")]
    pub fetched_leverage: IndexMap<String, FetchedLeverage>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "socketHealth")]
    pub socket_health: Option<Value>,
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    #[cfg(test)]
    use super::StoreData;
    use super::{
        ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage,
        IndexMap, Market, Order, OrderRiskProfile, OrderSide, Position, RequestOutcomeCounts,
        SliceAction, StoreDataLoaded, StoreOptions, StoreUpdateIds, TaskOrigin, Ticker, TwapStatus,
        Value,
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

    #[derive(serde::Deserialize)]
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

    impl<'de> serde::Deserialize<'de> for super::DisplayedPosition {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireDisplayedPosition::deserialize(crate::strict_decode::object(deserializer))
        }
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
        latency: f64,
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
        zk_link_config: Option<Value>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "userData")]
        user_data: Option<Value>,
        positions: Vec<Position>,
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
        socket_health: Option<Value>,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreData {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreData::deserialize(crate::strict_decode::object(deserializer))
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
        }
    }
}
