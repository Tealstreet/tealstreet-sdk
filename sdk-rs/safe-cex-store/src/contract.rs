//! Complete host-facing Store contract; source: packages/safe-cex/src/store/store.interface.ts.
//! ```compile_fail
//! use safe_cex_store::contract::strict_codecs::WireStoreUpdate;
//! ```
use crate::model_patch::{ModelPatch, PatchSource};
use crate::notification::StoreMutationError;
use indexmap::IndexMap;
use safe_cex_types::{
    ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage, Market,
    Order, OrderRiskProfile, OrderStatus, Position, RequestOutcome, RequestOutcomeCounts,
    StoreData, StoreDataLoaded, StoreOptions, StoreSubscriber, StoreUpdateIds, Ticker,
};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::cell::Ref;

#[derive(Debug, Clone, PartialEq)]
pub struct PackedTickerDelta {
    pub index: f64,
    pub mask: f64,
    pub values: Vec<f64>,
}
impl Serialize for PackedTickerDelta {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_seq(Some(2 + self.values.len()))?;
        tuple.serialize_element(&self.index)?;
        tuple.serialize_element(&self.mask)?;
        for value in &self.values {
            tuple.serialize_element(value)?;
        }
        tuple.end()
    }
}
impl<'de> Deserialize<'de> for PackedTickerDelta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<f64>::deserialize(deserializer)?;
        if values.len() < 2 {
            return Err(serde::de::Error::custom(
                "packed ticker delta needs index and mask",
            ));
        }
        Ok(Self {
            index: values[0],
            mask: values[1],
            values: values[2..].to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct StoreUpdateIdsPatch {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub positions: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub orders: Option<f64>,
}
impl From<StoreUpdateIds> for StoreUpdateIdsPatch {
    fn from(value: StoreUpdateIds) -> Self {
        Self {
            positions: Some(value.positions),
            orders: Some(value.orders),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreUpdate {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub latency: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub balance: Option<Balance>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub markets: Option<Vec<Market>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tickers: Option<Vec<Ticker>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub orders: Option<Vec<Order>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "starkConfig")]
    pub stark_config: Option<Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "zkLinkConfig")]
    pub zk_link_config: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "userData")]
    pub user_data: Option<Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub positions: Option<Vec<Position>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "displayedPositions")]
    pub displayed_positions: Option<Vec<DisplayedPosition>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "displayedMargin")]
    pub displayed_margin: Option<DisplayedMargin>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "ordersRisk")]
    pub orders_risk: Option<IndexMap<String, Vec<OrderRiskProfile>>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "activeChasers")]
    pub active_chasers: Option<Vec<ActiveChaser>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "activeTWAPs")]
    pub active_twaps: Option<Vec<ActiveTWAP>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub loaded: Option<StoreDataLoaded>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub options: Option<StoreOptions>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "updateIds")]
    pub update_ids: Option<StoreUpdateIdsPatch>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "messageHandlerStats")]
    pub message_handler_stats: Option<Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "requestStats")]
    pub request_stats: Option<RequestOutcomeCounts>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "fetchedLeverage")]
    pub fetched_leverage: Option<IndexMap<String, FetchedLeverage>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "socketHealth")]
    pub socket_health: Option<Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(rename = "tickerDeltas")]
    pub ticker_deltas: Option<Vec<PackedTickerDelta>>,
}

fn deserialize_present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

type Subscriber = StoreSubscriber;

/// Object-safe, local callbacks support native and browser hosts without an SDK projection.
pub trait Store {
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError>;
    fn reset(&self) -> Result<(), StoreMutationError>;
    fn subscribe(&self, callback: Subscriber) -> Box<dyn Fn()>;
    fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError>;
    fn remove_order(&self, id: &str) -> Result<(), StoreMutationError>;
    fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<(), StoreMutationError>;
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError>;
    fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError>;
    fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError>;
    fn update_order(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError>;
    fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError>;
    /// None retains an unmatched selector row so adaptive batch indexing keeps
    /// the original update count; it never matches a native string ID.
    fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError>;
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError>;
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError>;
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError>;
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError>;
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError>;
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError>;
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError>;
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError>;
    fn update_ticker(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError>;
    fn apply_packed_ticker_deltas(
        &self,
        deltas: Vec<PackedTickerDelta>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError>;
    fn update_market(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError>;
    fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError>;
    fn update_zk_link_config(
        &self,
        data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError>;
    fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError>;
    fn latency(&self) -> f64;
    fn balance(&self) -> Ref<'_, Balance>;
    /// Retain the current balance object without holding any Store borrow.
    /// Replacing/resetting the balance leaves previously returned objects alive.
    fn balance_handle(&self) -> std::sync::Arc<Balance>;
    fn markets(&self) -> Ref<'_, [Market]>;
    fn tickers(&self) -> Ref<'_, [Ticker]>;
    fn orders(&self) -> Ref<'_, [Order]>;
    fn orders_risk(&self) -> Ref<'_, IndexMap<String, Vec<OrderRiskProfile>>>;
    fn positions(&self) -> Ref<'_, [Position]>;
    fn displayed_positions(&self) -> Ref<'_, [DisplayedPosition]>;
    fn displayed_margin(&self) -> Option<Ref<'_, DisplayedMargin>>;
    fn active_chasers(&self) -> Ref<'_, [ActiveChaser]>;
    fn active_twaps(&self) -> Ref<'_, [ActiveTWAP]>;
    fn loaded(&self) -> Ref<'_, StoreDataLoaded>;
    fn update_ids(&self) -> Ref<'_, StoreUpdateIds>;
    fn options(&self) -> Ref<'_, StoreOptions>;
    fn message_handler_stats(&self) -> Option<Ref<'_, Value>>;
    fn stark_config(&self) -> Option<Ref<'_, Value>>;
    fn zk_link_config(&self) -> Option<Ref<'_, safe_cex_types::retained_json::RetainedJsonInput>>;
    fn user_data(&self) -> Option<Ref<'_, Value>>;
    fn tickers_by_id(&self) -> Ref<'_, IndexMap<String, Ticker>>;
    fn markets_by_id(&self) -> Ref<'_, IndexMap<String, Market>>;
    fn tickers_by_symbol(&self) -> Ref<'_, IndexMap<String, Ticker>>;
    /// Property lookup beyond the borrowed own-entry view. Stores with mutable
    /// dictionary prototypes override this stock-prototype implementation.
    fn lookup_ticker_by_symbol(&self, symbol: &str) -> crate::ticker_lookup::TickerSymbolLookup {
        let tickers = self.tickers_by_symbol();
        if let Some(ticker) = tickers.get(symbol) {
            crate::ticker_lookup::TickerSymbolLookup::OwnTicker(ticker.id.clone())
        } else {
            crate::ticker_lookup::stock(symbol)
        }
    }

    fn markets_by_symbol(&self) -> Ref<'_, IndexMap<String, Market>>;
    fn get_markets_version(&self) -> f64;
    fn get_tickers_version(&self) -> f64;
    fn fetched_leverage(&self) -> Ref<'_, IndexMap<String, FetchedLeverage>>;
    fn socket_health(&self) -> Option<Ref<'_, Value>>;
    fn update_socket_health(&self, data: &Map<String, Value>) -> Result<(), StoreMutationError>;
    fn request_stats(&self) -> Option<Ref<'_, RequestOutcomeCounts>>;
    fn record_request_outcome(&self, outcome: RequestOutcome);
    fn serialize(&self) -> StoreData;
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    #[cfg(test)]
    use super::StoreUpdate;
    use super::{
        ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage,
        IndexMap, Market, Order, OrderRiskProfile, PackedTickerDelta, Position,
        RequestOutcomeCounts, StoreDataLoaded, StoreOptions, StoreUpdateIdsPatch, Ticker, Value,
        deserialize_present,
    };
    #[cfg(test)]
    use serde::Deserialize;
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreUpdateIdsPatch")]
    struct WireStoreUpdateIdsPatch {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        positions: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        orders: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreUpdateIdsPatch {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreUpdateIdsPatch::deserialize(safe_cex_types::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreUpdate")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreUpdate {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        latency: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        balance: Option<Balance>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        markets: Option<Vec<Market>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tickers: Option<Vec<Ticker>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        orders: Option<Vec<Order>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "starkConfig")]
        stark_config: Option<Value>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "zkLinkConfig")]
        zk_link_config: Option<safe_cex_types::retained_json::RetainedJsonInput>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "userData")]
        user_data: Option<Value>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        positions: Option<Vec<Position>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "displayedPositions")]
        displayed_positions: Option<Vec<DisplayedPosition>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "displayedMargin")]
        displayed_margin: Option<DisplayedMargin>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "ordersRisk")]
        orders_risk: Option<IndexMap<String, Vec<OrderRiskProfile>>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "activeChasers")]
        active_chasers: Option<Vec<ActiveChaser>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "activeTWAPs")]
        active_twaps: Option<Vec<ActiveTWAP>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        loaded: Option<StoreDataLoaded>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        options: Option<StoreOptions>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "updateIds")]
        update_ids: Option<StoreUpdateIdsPatch>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "messageHandlerStats")]
        message_handler_stats: Option<Value>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "requestStats")]
        request_stats: Option<RequestOutcomeCounts>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "fetchedLeverage")]
        fetched_leverage: Option<IndexMap<String, FetchedLeverage>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "socketHealth")]
        socket_health: Option<Value>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        #[serde(rename = "tickerDeltas")]
        ticker_deltas: Option<Vec<PackedTickerDelta>>,
    }

    #[cfg(test)]
    #[test]
    fn complete_sequence_is_valid_for_wire_companion_but_not_public_contract() {
        let full = super::store_update_sequence();
        assert!(WireStoreUpdate::deserialize(full.clone()).is_ok());
        assert!(<StoreUpdate as Deserialize>::deserialize(full).is_err());
    }

    impl<'de> serde::Deserialize<'de> for super::StoreUpdate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct StoreRecordVisitor;
            impl<'de> serde::de::Visitor<'de> for StoreRecordVisitor {
                type Value = super::StoreUpdate;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a Store update record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    deserializer: D,
                ) -> Result<Self::Value, D::Error> {
                    WireStoreUpdate::deserialize(safe_cex_types::strict_decode::object(
                        deserializer,
                    ))
                }
            }
            deserializer
                .deserialize_newtype_struct(safe_cex_types::STORE_RECORD, StoreRecordVisitor)
        }
    }
}

#[cfg(test)]
fn store_update_sequence() -> Value {
    serde_json::json!([1,{"used":0,"free":0,"total":0,"upnl":0},[],[],[],{}, {}, {},[],[],{"longExposure":0,"shortExposure":0,"totalNotional":0}, {},[],[],{"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false},{"isHedged":false},{"positions":0,"orders":0},{},{"ok":0,"rateLimit":0,"auth":0,"network":0,"other":0},{},{},[]])
}

#[cfg(test)]
mod strict_store_update_tests {
    use super::*;
    use serde::de::IntoDeserializer;
    use serde_json::json;
    #[test]
    fn worker_update_requires_object_even_when_every_field_is_optional() {
        assert!(StoreUpdate::deserialize(json!([]).into_deserializer()).is_err());
        assert!(StoreUpdateIdsPatch::deserialize(json!([1, 2]).into_deserializer()).is_err());
        assert!(
            StoreUpdateIdsPatch::deserialize(json!({"positions":1,"orders":2}).into_deserializer())
                .is_ok()
        );
        assert!(StoreUpdate::deserialize(json!({}).into_deserializer()).is_ok());
        let full = store_update_sequence();
        assert!(<StoreUpdate as Deserialize>::deserialize(full).is_err());
        assert!(<StoreUpdate as Deserialize>::deserialize(json!({"balance":[0,0,0,0]})).is_err());
        assert!(<StoreUpdate as Deserialize>::deserialize(json!({"updateIds":[1,2]})).is_err());
    }
}
