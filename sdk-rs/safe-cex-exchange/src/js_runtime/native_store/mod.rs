//! The production Store's sole canonical native value graph.
mod graph;

use crate::js_runtime::value::{Js, Thrown, from_typed, to_typed_preserving_undefined};
use indexmap::IndexMap;
use safe_cex_store::{
    BalanceHandle, BalanceReference, PackedTickerDelta, Store, StoreMutationError, StoreUpdate,
    model_patch::{ModelPatch, PatchModel, PatchSource},
};
use safe_cex_types::{
    ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage, Market,
    Order, OrderRiskProfile, OrderStatus, Position, PublicDataStore, PublicDataSubscriber,
    RequestOutcome, RequestOutcomeCounts, StoreData, StoreDataLoaded, StoreOptions, StoreReadError,
    StoreReadPhase, StoreSubscriber, StoreUpdateIds, Ticker, retained_json,
};
use serde_json::Value;
use std::{cell::OnceCell, rc::Rc};

fn property(error: Thrown) -> StoreReadError {
    StoreReadError::new(StoreReadPhase::Property, Rc::new(error))
}

impl From<StoreReadError> for Thrown {
    fn from(error: StoreReadError) -> Self {
        read_thrown(error)
    }
}

fn project<T: serde::de::DeserializeOwned>(value: &Js) -> Result<T, StoreReadError> {
    to_typed_preserving_undefined(value)
        .map_err(|cause| StoreReadError::new(StoreReadPhase::Projection, Rc::new(cause)))
}

fn read_thrown(error: StoreReadError) -> Thrown {
    if let Some(cause) = error.native_cause().downcast_ref::<Thrown>() {
        return Thrown {
            value: cause.value.clone(),
            native: Some(Rc::new(crate::runtime::ExchangeError::from(error.clone()))),
        };
    }
    Thrown::from(crate::runtime::ExchangeError::from(error))
}

fn mutation_thrown(error: StoreMutationError) -> Thrown {
    match error {
        StoreMutationError::Access(error) => read_thrown(error),
        notification @ StoreMutationError::Notification(_) => {
            let mut native = crate::runtime::ExchangeError::from(notification.clone());
            // Materialize the same value before attaching the complete typed
            // boundary. The flattened metadata alone cannot recover nested IDs.
            let mut thrown = Thrown::from(native.clone());
            native.origin = Some(crate::runtime::ExchangeErrorOrigin::new(notification));
            thrown.native = Some(Rc::new(native));
            thrown
        }
        other => Thrown::from(crate::runtime::ExchangeError::from(other)),
    }
}

fn mutation_error(error: Thrown) -> StoreMutationError {
    if let Some(native) = &error.native {
        if let Some(origin) = &native.origin {
            if let Some(error) = origin.error().downcast_ref::<StoreMutationError>() {
                return error.clone();
            }
            if let Some(error) = origin.error().downcast_ref::<StoreReadError>() {
                return error.clone().into();
            }
        }
        if let (Some(id), Some(cause)) = (
            native.store_subscriber_ids.last(),
            native.store_subscriber_cause.as_ref(),
        ) {
            return safe_cex_store::StoreNotificationError::from_subscriber(*id, cause.clone())
                .into();
        }
    }
    property(error).into()
}

fn encode<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, StoreMutationError> {
    from_typed(value)
        .map_err(|error| StoreReadError::new(StoreReadPhase::Projection, Rc::new(error)).into())
}

fn selector(id: &str) -> Js {
    Js::object(vec![("id", Js::str(id))])
}
fn selectors(ids: &[&str]) -> Js {
    Js::array(ids.iter().map(|id| selector(id)).collect())
}

fn admitted_patch<T: serde::de::DeserializeOwned + serde::Serialize + Clone + PatchModel>(
    target: &Js,
    changes: &dyn PatchSource,
) -> Result<Js, StoreMutationError> {
    let mut candidate: T = project(target)?;
    safe_cex_store::model_patch::apply(&mut candidate, changes)?;
    let candidate = encode(&candidate)?;
    let out = Js::object(vec![]);
    let mut fields = vec![];
    changes.visit(&mut |field, _| {
        fields.push(field.to_owned());
        Ok(())
    })?;
    for field in fields {
        if candidate.has_own(&field) {
            out.define(&field, candidate.get(&field).map_err(mutation_error)?)
                .map_err(mutation_error)?;
        }
    }
    Ok(out)
}

/// A native facade requires the exact material-owner capability. No typed fallback.
pub fn facade(store: Rc<dyn Store>) -> Result<Js, Thrown> {
    let graph = store
        .native_owner()
        .downcast::<NativeOwner>()
        .map_err(|_| Thrown::type_error("Store does not expose the canonical native owner"))?;
    graph.facade()
}

struct CapturedTicker(Js);
impl safe_cex_store::ticker_lookup::TickerLookup for CapturedTicker {
    fn is_truthy(&self) -> bool {
        self.0.truthy()
    }
    fn update_id(&self) -> Result<Option<String>, StoreReadError> {
        if !self.0.truthy() {
            return Ok(None);
        }
        let id = self.0.get("id").map_err(property)?;
        let units = id.to_js_string().map_err(property)?;
        String::from_utf16(&units).map(Some).map_err(|_| {
            property(Thrown::type_error(
                "Ticker id is outside the UTF-8 identifier boundary",
            ))
        })
    }
}

struct CapturedBalance(Js);

impl BalanceReference for CapturedBalance {
    fn read(&self) -> Result<Balance, StoreReadError> {
        project(&self.0)
    }

    fn native_value(&self) -> Rc<dyn std::any::Any> {
        Rc::new(self.0.clone())
    }
}

/// No balance fields are read while capturing this native referent.
pub fn balance_reference(value: Js) -> BalanceHandle {
    BalanceHandle::new(Rc::new(CapturedBalance(value)))
}

/// Preserve the captured balance referent when native polling commits its result.
/// This is the source's `update({ balance, loaded })`, in the same property order.
pub fn update_balance_and_loaded(
    store: &dyn Store,
    balance: &BalanceHandle,
    loaded: &StoreDataLoaded,
) -> Result<(), StoreMutationError> {
    let value = balance.native_value().downcast::<Js>().map_err(|_| {
        property(Thrown::type_error(
            "Balance handle does not expose a native value",
        ))
    })?;
    let owner = store
        .native_owner()
        .downcast::<NativeOwner>()
        .map_err(|_| {
            property(Thrown::type_error(
                "Store does not expose the canonical native owner",
            ))
        })?;
    let changes = Js::object(vec![
        ("balance", (*value).clone()),
        ("loaded", encode(loaded)?),
    ]);
    owner.graph.update(&changes, false).map_err(mutation_error)
}

// This capability retains the facade outside Graph. The facade's resource and
// methods capture only Graph, so neither can retain this cache or create a cycle.
struct NativeOwner {
    graph: Rc<graph::Graph>,
    facade: OnceCell<Js>,
}

impl NativeOwner {
    fn facade(&self) -> Result<Js, Thrown> {
        if let Some(value) = self.facade.get() {
            return Ok(value.clone());
        }
        // Facade construction installs descriptors without invoking user code.
        // Failed construction leaves the cell empty and the original error visible.
        let value = self.graph.facade()?;
        let _ = self.facade.set(value.clone());
        Ok(value)
    }
}

/// The Store and its capability share one graph; rows never retain the owner.
pub struct DefaultStore {
    graph: Rc<graph::Graph>,
    owner: Rc<NativeOwner>,
    pub now: std::sync::Arc<dyn Fn() -> f64 + Send + Sync>,
}

impl Default for DefaultStore {
    fn default() -> Self {
        let graph = Rc::new(graph::Graph::new());
        let now = graph.now.borrow().clone();
        let owner = Rc::new(NativeOwner {
            graph: graph.clone(),
            facade: OnceCell::new(),
        });
        Self { graph, owner, now }
    }
}

impl DefaultStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn prepare(&self) {
        *self.graph.now.borrow_mut() = self.now.clone();
    }

    pub fn native_value(&self, name: &str) -> Result<Js, Thrown> {
        self.graph.get(name)
    }

    pub fn update_native(&self, changes: &Js, from_worker: bool) -> Result<(), Thrown> {
        self.prepare();
        self.graph.update(changes, from_worker)
    }

    pub fn facade(&self) -> Result<Js, Thrown> {
        self.prepare();
        self.owner.facade()
    }

    pub fn subscribe(
        &self,
        callback: std::sync::Arc<
            dyn Fn(
                &safe_cex_types::StoreData,
                bool,
            ) -> Result<(), safe_cex_types::StoreSubscriberError>,
        >,
    ) -> Box<dyn Fn()> {
        self.graph.subscribe_typed(callback)
    }

    pub fn record_request_outcome(
        &self,
        outcome: safe_cex_types::RequestOutcome,
    ) -> Result<(), StoreMutationError> {
        let key = match outcome {
            safe_cex_types::RequestOutcome::Ok => "ok",
            safe_cex_types::RequestOutcome::RateLimit => "rateLimit",
            safe_cex_types::RequestOutcome::Auth => "auth",
            safe_cex_types::RequestOutcome::Network => "network",
            safe_cex_types::RequestOutcome::Other => "other",
        };
        self.graph
            .record_request_outcome(key)
            .map_err(mutation_error)
    }

    pub fn balance_handle(&self) -> Result<BalanceHandle, StoreReadError> {
        self.graph
            .get("balance")
            .map(balance_reference)
            .map_err(property)
    }

    fn read<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, StoreReadError> {
        project(&self.graph.get(name).map_err(property)?)
    }

    fn invoke(&self, name: &str, args: Vec<Js>) -> Result<(), StoreMutationError> {
        self.prepare();
        self.graph
            .invoke(name, &args)
            .map(|_| ())
            .map_err(mutation_error)
    }

    fn typed_patch(
        &self,
        key: &str,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        let target = if key == "tickers" {
            self.graph
                .get("tickersById")
                .and_then(|map| map.get(id))
                .map_err(mutation_error)?
        } else if key == "markets" {
            self.graph
                .market_for_id(&Js::str(id))
                .map_err(mutation_error)?
        } else {
            self.graph
                .find_row(key, &Js::str(id))
                .map_err(mutation_error)?
        };
        if !target.truthy() {
            return Ok(());
        }
        let changes = match key {
            "tickers" => admitted_patch::<Ticker>(&target, changes)?,
            "markets" => admitted_patch::<Market>(&target, changes)?,
            "orders" => admitted_patch::<Order>(&target, changes)?,
            "positions" => admitted_patch::<Position>(&target, changes)?,
            _ => {
                return Err(StoreMutationError::InvalidOperation(
                    "Unknown typed Store patch target",
                ));
            }
        };
        let method = match key {
            "tickers" => "updateTicker",
            "markets" => "updateMarket",
            "orders" => "updateOrder",
            _ => "updatePosition",
        };
        self.invoke(method, vec![selector(id), changes])
    }
}

impl Store for DefaultStore {
    fn native_owner(&self) -> Rc<dyn std::any::Any> {
        self.prepare();
        self.owner.clone()
    }
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError> {
        self.invoke("update", vec![encode(&changes)?, Js::Bool(from_worker)])
    }
    fn reset(&self) -> Result<(), StoreMutationError> {
        self.invoke("reset", vec![])
    }
    fn subscribe_native(
        &self,
        callback: safe_cex_store::contract::NativeStoreSubscriber,
    ) -> Box<dyn Fn()> {
        self.graph.subscribe_native_callback(callback.into())
    }
    fn subscribe(&self, callback: StoreSubscriber) -> Box<dyn Fn()> {
        self.graph.subscribe_typed(callback.into())
    }
    fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError> {
        self.invoke("setSetting", vec![Js::str(key), Js::Bool(value)])
    }
    fn remove_order(&self, id: &str) -> Result<(), StoreMutationError> {
        self.invoke("removeOrder", vec![selector(id)])
    }
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        self.invoke("removeOrders", vec![selectors(ids)])
    }
    fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<(), StoreMutationError> {
        self.invoke("updateOrderStatus", vec![selector(id), encode(&status)?])
    }
    fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.invoke("updateOrdersStatus", vec![selectors(ids), encode(&status)?])
    }
    fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        if !matches!(status, OrderStatus::Closed | OrderStatus::Canceled) {
            return Err(StoreMutationError::InvalidOperation(
                "terminal status must be closed or canceled",
            ));
        }
        self.invoke("markOrdersTerminal", vec![selectors(ids), encode(&status)?])
    }
    fn update_order(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.typed_patch("orders", id, changes)
    }
    fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.typed_patch("positions", id, changes)
    }
    fn update_ticker(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.typed_patch("tickers", id, changes)
    }
    fn update_market(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.typed_patch("markets", id, changes)
    }
    fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        if updates.is_empty() {
            return Ok(());
        }
        let mut admitted = vec![];
        for (id, changes) in updates {
            let id = id.as_ref().map_or(Js::Undefined, |id| Js::str(id));
            let target = self
                .graph
                .find_row("positions", &id)
                .map_err(mutation_error)?;
            let changes = if target.is_undefined() {
                Js::object(vec![])
            } else {
                admitted_patch::<Position>(&target, changes)?
            };
            admitted.push(Js::array(vec![Js::object(vec![("id", id)]), changes]));
        }
        self.invoke("updatePositions", vec![Js::array(admitted)])
    }
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.invoke("addOrder", vec![encode(&order)?])
    }
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdateOrder", vec![encode(&order)?])
    }
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdateOrders", vec![encode(&orders)?])
    }
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdateOrdersFull", vec![encode(&orders)?])
    }
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        self.invoke(
            "setOrdersForSymbol",
            vec![Js::str(symbol), encode(&orders)?],
        )
    }
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdatePosition", vec![encode(&position)?])
    }
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdatePositions", vec![encode(&positions)?])
    }
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        self.invoke("addOrUpdatePositionsFull", vec![encode(&positions)?])
    }
    fn apply_packed_ticker_deltas(
        &self,
        deltas: Vec<PackedTickerDelta>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.invoke(
            "applyPackedTickerDeltas",
            vec![encode(&deltas)?, Js::Bool(from_worker)],
        )
    }
    fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.invoke(
            "updateStarkConfig",
            vec![match data {
                Some(data) => encode(&data)?,
                None => Js::Undefined,
            }],
        )
    }
    fn update_zk_link_config(
        &self,
        data: Option<retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError> {
        self.invoke(
            "updateZkLinkConfig",
            vec![match data {
                Some(data) => encode(&data)?,
                None => Js::Undefined,
            }],
        )
    }
    fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.invoke(
            "updateUserData",
            vec![match data {
                Some(data) => encode(&data)?,
                None => Js::Undefined,
            }],
        )
    }
    fn update_socket_health(
        &self,
        data: &safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<(), StoreMutationError> {
        self.invoke("updateSocketHealth", vec![encode(data)?])
    }
    fn latency(&self) -> Result<f64, StoreReadError> {
        self.read("latency")
    }
    fn balance(&self) -> Result<Balance, StoreReadError> {
        self.read("balance")
    }
    fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        self.read("markets")
    }
    fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        self.read("tickers")
    }
    fn orders(&self) -> Result<Vec<Order>, StoreReadError> {
        self.read("orders")
    }
    fn orders_risk(&self) -> Result<IndexMap<String, Vec<OrderRiskProfile>>, StoreReadError> {
        self.read("ordersRisk")
    }
    fn positions(&self) -> Result<Vec<Position>, StoreReadError> {
        self.read("positions")
    }
    fn displayed_positions(&self) -> Result<Vec<DisplayedPosition>, StoreReadError> {
        self.read("displayedPositions")
    }
    fn displayed_margin(&self) -> Result<Option<DisplayedMargin>, StoreReadError> {
        self.read("displayedMargin")
    }
    fn active_chasers(&self) -> Result<Vec<ActiveChaser>, StoreReadError> {
        self.read("activeChasers")
    }
    fn active_twaps(&self) -> Result<Vec<ActiveTWAP>, StoreReadError> {
        self.read("activeTWAPs")
    }
    fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        self.read("loaded")
    }
    fn update_ids(&self) -> Result<StoreUpdateIds, StoreReadError> {
        self.read("updateIds")
    }
    fn options(&self) -> Result<StoreOptions, StoreReadError> {
        self.read("options")
    }
    fn message_handler_stats(&self) -> Result<Option<Value>, StoreReadError> {
        self.read("messageHandlerStats")
    }
    fn stark_config(&self) -> Result<Option<Value>, StoreReadError> {
        self.read("starkConfig")
    }
    fn zk_link_config(&self) -> Result<Option<retained_json::RetainedJsonInput>, StoreReadError> {
        let state = self.graph.state();
        let present = state.has_own("zkLinkConfig");
        let value = state.get("zkLinkConfig").map_err(property)?;
        if !present && value.is_undefined() {
            Ok(None)
        } else {
            // The outer presence is independent of the retained payload: an
            // own undefined (or null) remains a present opaque config value.
            project(&value).map(Some)
        }
    }
    fn user_data(&self) -> Result<Option<Value>, StoreReadError> {
        self.read("userData")
    }
    fn tickers_by_id(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        self.read("tickersById")
    }
    fn markets_by_id(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        self.read("marketsById")
    }
    fn tickers_by_symbol(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        self.read("tickersBySymbol")
    }
    fn markets_by_symbol(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        self.read("marketsBySymbol")
    }
    fn fetched_leverage(&self) -> Result<IndexMap<String, FetchedLeverage>, StoreReadError> {
        self.read("fetchedLeverage")
    }
    fn socket_health(
        &self,
    ) -> Result<Option<safe_cex_types::retained_json::RetainedJsonInput>, StoreReadError> {
        self.read("socketHealth")
    }
    fn request_stats(&self) -> Result<Option<RequestOutcomeCounts>, StoreReadError> {
        self.read("requestStats")
    }
    fn balance_handle(&self) -> Result<BalanceHandle, StoreReadError> {
        DefaultStore::balance_handle(self)
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) -> Result<(), StoreMutationError> {
        DefaultStore::record_request_outcome(self, outcome)
    }
    fn get_markets_version(&self) -> f64 {
        self.graph.market_version.get()
    }
    fn get_tickers_version(&self) -> f64 {
        self.graph.ticker_version.get()
    }
    fn lookup_ticker_by_symbol(
        &self,
        symbol: &str,
    ) -> Result<Rc<dyn safe_cex_store::ticker_lookup::TickerLookup>, StoreReadError> {
        let value = self
            .graph
            .get("tickersBySymbol")
            .and_then(|map| map.get(symbol))
            .map_err(property)?;
        Ok(Rc::new(CapturedTicker(value)))
    }
    fn serialize(&self) -> Result<StoreData, StoreReadError> {
        project(&self.graph.serialize().map_err(property)?)
    }
}

impl PublicDataStore for DefaultStore {
    // The public callback ABI is an Arc; callbacks stay on this local runtime,
    // and subscriber removal compares the original Arc identity.
    #[allow(clippy::arc_with_non_send_sync)]
    fn subscribe(&self, callback: PublicDataSubscriber) -> Box<dyn Fn()> {
        self.graph
            .subscribe_typed(std::sync::Arc::new(move |data, _| callback(data)))
    }
    fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        self.read("tickers")
    }
    fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        self.read("markets")
    }
    fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        self.read("loaded")
    }
}

impl DefaultStore {
    pub fn latency(&self) -> Result<f64, StoreReadError> {
        Store::latency(self)
    }
    pub fn balance(&self) -> Result<Balance, StoreReadError> {
        Store::balance(self)
    }
    pub fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        Store::markets(self)
    }
    pub fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        Store::tickers(self)
    }
    pub fn orders(&self) -> Result<Vec<Order>, StoreReadError> {
        Store::orders(self)
    }
    pub fn orders_risk(&self) -> Result<IndexMap<String, Vec<OrderRiskProfile>>, StoreReadError> {
        Store::orders_risk(self)
    }
    pub fn positions(&self) -> Result<Vec<Position>, StoreReadError> {
        Store::positions(self)
    }
    pub fn displayed_positions(&self) -> Result<Vec<DisplayedPosition>, StoreReadError> {
        Store::displayed_positions(self)
    }
    pub fn displayed_margin(&self) -> Result<Option<DisplayedMargin>, StoreReadError> {
        Store::displayed_margin(self)
    }
    pub fn active_chasers(&self) -> Result<Vec<ActiveChaser>, StoreReadError> {
        Store::active_chasers(self)
    }
    pub fn active_twaps(&self) -> Result<Vec<ActiveTWAP>, StoreReadError> {
        Store::active_twaps(self)
    }
    pub fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        Store::loaded(self)
    }
    pub fn update_ids(&self) -> Result<StoreUpdateIds, StoreReadError> {
        Store::update_ids(self)
    }
    pub fn options(&self) -> Result<StoreOptions, StoreReadError> {
        Store::options(self)
    }
    pub fn message_handler_stats(&self) -> Result<Option<Value>, StoreReadError> {
        Store::message_handler_stats(self)
    }
    pub fn stark_config(&self) -> Result<Option<Value>, StoreReadError> {
        Store::stark_config(self)
    }
    pub fn zk_link_config(
        &self,
    ) -> Result<Option<retained_json::RetainedJsonInput>, StoreReadError> {
        Store::zk_link_config(self)
    }
    pub fn user_data(&self) -> Result<Option<Value>, StoreReadError> {
        Store::user_data(self)
    }
    pub fn tickers_by_id(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Store::tickers_by_id(self)
    }
    pub fn markets_by_id(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        Store::markets_by_id(self)
    }
    pub fn tickers_by_symbol(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Store::tickers_by_symbol(self)
    }
    pub fn markets_by_symbol(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        Store::markets_by_symbol(self)
    }
    pub fn fetched_leverage(&self) -> Result<IndexMap<String, FetchedLeverage>, StoreReadError> {
        Store::fetched_leverage(self)
    }
    pub fn socket_health(
        &self,
    ) -> Result<Option<safe_cex_types::retained_json::RetainedJsonInput>, StoreReadError> {
        Store::socket_health(self)
    }
    pub fn request_stats(&self) -> Result<Option<RequestOutcomeCounts>, StoreReadError> {
        Store::request_stats(self)
    }
    pub fn serialize(&self) -> Result<StoreData, StoreReadError> {
        Store::serialize(self)
    }
    pub fn update_full(
        &self,
        changes: StoreUpdate,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        Store::update(self, changes, from_worker)
    }
    pub fn pack_ticker_deltas<P: PatchSource>(
        deltas: &[(f64, P)],
    ) -> Result<
        safe_cex_store::store::PackedTickerDeltas,
        safe_cex_store::model_patch::StorePatchError,
    > {
        safe_cex_store::store::pack_ticker_deltas(deltas)
    }
    pub fn consume_ticker_deltas(&self) -> Vec<(f64, serde_json::Map<String, Value>)> {
        vec![]
    }
    pub fn audit_positions(&self, remove_dupes: bool) -> Result<(), StoreMutationError> {
        self.graph
            .audit_positions(remove_dupes)
            .map_err(mutation_error)
    }
    pub fn create_empty_position_from(position: Position) -> Result<Position, StoreReadError> {
        let value = from_typed(&position).map_err(property)?;
        project(&graph::Graph::empty_position(&value).map_err(property)?)
    }
    pub fn apply_ticker_deltas<P: PatchSource>(
        &self,
        deltas: &[(f64, P)],
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        let mut admitted = vec![];
        for (index, changes) in deltas {
            let mut nonempty = false;
            changes.visit(&mut |key, _| {
                nonempty |= key != "info";
                Ok(())
            })?;
            if !nonempty {
                continue;
            }
            let rows = self.graph.get("tickers").map_err(mutation_error)?;
            if *index <= -1.0
                || *index
                    >= rows
                        .get("length")
                        .map_err(mutation_error)?
                        .to_number()
                        .map_err(mutation_error)?
            {
                continue;
            }
            if !index.is_finite() || index.fract() != 0.0 || *index < 0.0 {
                return Err(safe_cex_store::model_patch::StorePatchError::invalid_index().into());
            }
            let target = rows
                .get(&(*index as usize).to_string())
                .map_err(mutation_error)?;
            let patch = admitted_patch::<Ticker>(&target, changes)?;
            patch
                .delete(&crate::js_runtime::value::utf16("info"))
                .map_err(mutation_error)?;
            admitted.push(Js::array(vec![Js::Number(*index), patch]));
        }
        self.invoke(
            "applyTickerDeltas",
            vec![Js::array(admitted), Js::Bool(from_worker)],
        )
    }
}
