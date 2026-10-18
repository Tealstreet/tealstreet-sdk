//! Test-only implementation of the actual shared Store contract.
use indexmap::IndexMap;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{
    PackedTickerDelta, Store, StoreMutationError, StoreNotificationError, StoreUpdate,
    model_patch::{ModelPatch, PatchSource},
};
use safe_cex_types::*;
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
type Subscriber = Rc<dyn Fn(&StoreData, bool) -> Result<(), StoreSubscriberError>>;
pub struct TestStore {
    pub data: RefCell<StoreData>,
    subscribers: Rc<RefCell<Vec<(u64, Subscriber)>>>,
    next_subscriber_id: Cell<u64>,
}
impl Default for TestStore {
    fn default() -> Self {
        Self {data:RefCell::new(serde_json::from_value(json!({"latency":0,"balance":{"used":0,"free":0,"total":0,"upnl":0},"markets":[],"tickers":[],"orders":[],"positions":[],"displayedPositions":[],"ordersRisk":{},"activeChasers":[],"activeTWAPs":[],"loaded":{"markets":false,"tickers":false,"positions":false,"orders":false,"balance":false},"options":{"isHedged":false},"updateIds":{"orders":0,"positions":0},"fetchedLeverage":{},"requestStats":{"ok":0,"rateLimit":0,"auth":0,"network":0,"other":0}})).unwrap()),subscribers:Rc::default(),next_subscriber_id:Cell::new(0)}
    }
}
impl TestStore {
    pub fn new() -> Self {
        Self::default()
    }
    fn notify(&self, from_worker: bool) -> Result<(), StoreMutationError> {
        let data = self.serialize()?;
        let callbacks = self.subscribers.borrow().clone();
        for (id, callback) in callbacks {
            callback(&data, from_worker)
                .map_err(|error| StoreNotificationError::from_subscriber(id, error))?;
        }
        Ok(())
    }
    fn orders_mutate(
        &self,
        change: impl FnOnce(&mut DefaultStore) -> Result<(), StoreMutationError>,
    ) -> Result<(), StoreMutationError> {
        let mut engine = DefaultStore::new();
        engine.add_or_update_orders_full(self.data.borrow().orders.clone())?;
        change(&mut engine)?;
        self.data.borrow_mut().orders = engine.orders()?;
        self.notify(false)
    }
}
impl Store for TestStore {
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError> {
        let mut snapshot = serde_json::to_value(self.serialize()?).unwrap();
        let changes = serde_json::to_value(changes).unwrap();
        for (key, value) in changes.as_object().unwrap() {
            snapshot
                .as_object_mut()
                .unwrap()
                .insert(key.clone(), value.clone());
        }
        *self.data.borrow_mut() = serde_json::from_value(snapshot).unwrap();
        self.notify(from_worker)
    }
    fn reset(&self) -> Result<(), StoreMutationError> {
        *self.data.borrow_mut() = Self::default().data.into_inner();
        self.notify(false)
    }
    fn subscribe(&self, callback: StoreSubscriber) -> Box<dyn Fn()> {
        let callback: Subscriber = Rc::from(callback);
        let id = self.next_subscriber_id.get();
        self.next_subscriber_id.set(id + 1);
        self.subscribers.borrow_mut().push((id, callback.clone()));
        let weak = Rc::downgrade(&self.subscribers);
        Box::new(move || {
            if let Some(rows) = weak.upgrade() {
                rows.borrow_mut().retain(|row| row.0 != id);
            }
        })
    }
    fn set_setting(&self, _key: &str, _value: bool) -> Result<(), StoreMutationError> {
        panic!("TestStore::set_setting is not exercised by this fixture")
    }
    fn remove_order(&self, _id: &str) -> Result<(), StoreMutationError> {
        panic!("TestStore::remove_order is not exercised by this fixture")
    }
    fn update_order_status(
        &self,
        _id: &str,
        _status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_order_status is not exercised by this fixture")
    }
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.remove_orders(ids))
    }
    fn update_orders_status(
        &self,
        _ids: &[&str],
        _status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_orders_status is not exercised by this fixture")
    }
    fn mark_orders_terminal(
        &self,
        _ids: &[&str],
        _status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::mark_orders_terminal is not exercised by this fixture")
    }
    fn update_order(
        &self,
        _id: &str,
        _changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_order is not exercised by this fixture")
    }
    fn update_position(
        &self,
        _id: &str,
        _changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_position is not exercised by this fixture")
    }
    fn update_positions(
        &self,
        _updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_positions is not exercised by this fixture")
    }
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        self.add_or_update_positions(vec![position])
    }
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError> {
        let mut data = self.data.borrow_mut();
        for position in positions {
            if let Some(existing) = data.positions.iter_mut().find(|p| p.id == position.id) {
                *existing = position;
            } else {
                data.positions.push(position);
            }
        }
        drop(data);
        self.notify(false)
    }
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        self.data.borrow_mut().positions = positions;
        self.notify(false)
    }
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.add_or_update_orders(vec![order]))
    }
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.add_or_update_orders(vec![order]))
    }
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.add_or_update_orders(orders))
    }
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.add_or_update_orders_full(orders))
    }
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        self.orders_mutate(|engine| engine.set_orders_for_symbol(symbol, orders))
    }
    fn update_ticker(
        &self,
        _id: &str,
        _changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_ticker is not exercised by this fixture")
    }
    fn apply_packed_ticker_deltas(
        &self,
        _deltas: Vec<PackedTickerDelta>,
        _from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::apply_packed_ticker_deltas is not exercised by this fixture")
    }
    fn update_market(
        &self,
        _id: &str,
        _changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_market is not exercised by this fixture")
    }
    fn update_stark_config(&self, _data: Option<Value>) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_stark_config is not exercised by this fixture")
    }
    fn update_zk_link_config(
        &self,
        _data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_zk_link_config is not exercised by this fixture")
    }
    fn update_user_data(&self, _data: Option<Value>) -> Result<(), StoreMutationError> {
        panic!("TestStore::update_user_data is not exercised by this fixture")
    }
    fn native_owner(&self) -> Rc<dyn std::any::Any> {
        // This controlled typed fixture deliberately has no native graph.
        // The real native facade must refuse it, rather than copying its data.
        Rc::new(())
    }
    fn balance_handle(&self) -> Result<safe_cex_store::BalanceHandle, StoreReadError> {
        panic!("TestStore::balance_handle is not exercised by this fixture")
    }
    fn lookup_ticker_by_symbol(
        &self,
        _: &str,
    ) -> Result<Rc<dyn safe_cex_store::ticker_lookup::TickerLookup>, StoreReadError> {
        panic!("TestStore::lookup_ticker_by_symbol is not exercised by this fixture")
    }
    fn latency(&self) -> Result<f64, StoreReadError> {
        Ok(self.data.borrow().latency.clone())
    }
    fn balance(&self) -> Result<Balance, StoreReadError> {
        Ok(self.data.borrow().balance.clone())
    }
    fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        Ok(self.data.borrow().markets.clone())
    }
    fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        Ok(self.data.borrow().tickers.clone())
    }
    fn orders(&self) -> Result<Vec<Order>, StoreReadError> {
        Ok(self.data.borrow().orders.clone())
    }
    fn orders_risk(&self) -> Result<IndexMap<String, Vec<OrderRiskProfile>>, StoreReadError> {
        Ok(self.data.borrow().orders_risk.clone())
    }
    fn positions(&self) -> Result<Vec<Position>, StoreReadError> {
        Ok(self.data.borrow().positions.clone())
    }
    fn displayed_positions(&self) -> Result<Vec<DisplayedPosition>, StoreReadError> {
        Ok(self.data.borrow().displayed_positions.clone())
    }
    fn displayed_margin(&self) -> Result<Option<DisplayedMargin>, StoreReadError> {
        Ok(self.data.borrow().displayed_margin.clone())
    }
    fn active_chasers(&self) -> Result<Vec<ActiveChaser>, StoreReadError> {
        Ok(self.data.borrow().active_chasers.clone())
    }
    fn active_twaps(&self) -> Result<Vec<ActiveTWAP>, StoreReadError> {
        Ok(self.data.borrow().active_twaps.clone())
    }
    fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        Ok(self.data.borrow().loaded.clone())
    }
    fn update_ids(&self) -> Result<StoreUpdateIds, StoreReadError> {
        Ok(self.data.borrow().update_ids.clone())
    }
    fn options(&self) -> Result<StoreOptions, StoreReadError> {
        Ok(self.data.borrow().options.clone())
    }
    fn message_handler_stats(&self) -> Result<Option<Value>, StoreReadError> {
        Ok(self.data.borrow().message_handler_stats.clone())
    }
    fn stark_config(&self) -> Result<Option<Value>, StoreReadError> {
        Ok(self.data.borrow().stark_config.clone())
    }
    fn zk_link_config(
        &self,
    ) -> Result<Option<safe_cex_types::retained_json::RetainedJsonInput>, StoreReadError> {
        Ok(self.data.borrow().zk_link_config.clone())
    }
    fn user_data(&self) -> Result<Option<Value>, StoreReadError> {
        Ok(self.data.borrow().user_data.clone())
    }
    fn fetched_leverage(&self) -> Result<IndexMap<String, FetchedLeverage>, StoreReadError> {
        Ok(self.data.borrow().fetched_leverage.clone())
    }
    fn socket_health(&self) -> Result<Option<Value>, StoreReadError> {
        Ok(self.data.borrow().socket_health.clone())
    }
    fn tickers_by_id(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Ok(self
            .data
            .borrow()
            .tickers
            .iter()
            .cloned()
            .map(|row| (row.id.clone(), row))
            .collect())
    }
    fn tickers_by_symbol(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Ok(self
            .data
            .borrow()
            .tickers
            .iter()
            .cloned()
            .map(|row| (row.symbol.clone(), row))
            .collect())
    }
    fn markets_by_id(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        Ok(self
            .data
            .borrow()
            .markets
            .iter()
            .cloned()
            .map(|row| (row.id.clone(), row))
            .collect())
    }
    fn markets_by_symbol(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        Ok(self
            .data
            .borrow()
            .markets
            .iter()
            .cloned()
            .map(|row| (row.symbol.clone(), row))
            .collect())
    }
    fn get_markets_version(&self) -> f64 {
        0.0
    }
    fn get_tickers_version(&self) -> f64 {
        0.0
    }
    fn update_socket_health(&self, data: &Map<String, Value>) -> Result<(), StoreMutationError> {
        let mut state = self.data.borrow_mut();
        let health = state.socket_health.get_or_insert_with(|| json!({}));
        health.as_object_mut().unwrap().extend(data.clone());
        drop(state);
        self.notify(false)
    }
    fn request_stats(&self) -> Result<Option<RequestOutcomeCounts>, StoreReadError> {
        Ok(self.data.borrow().request_stats.clone())
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) -> Result<(), StoreMutationError> {
        if let Some(stats) = &mut self.data.borrow_mut().request_stats {
            *match outcome {
                RequestOutcome::Ok => &mut stats.ok,
                RequestOutcome::RateLimit => &mut stats.rate_limit,
                RequestOutcome::Auth => &mut stats.auth,
                RequestOutcome::Network => &mut stats.network,
                RequestOutcome::Other => &mut stats.other,
            } += 1.;
        }
        Ok(())
    }
    fn serialize(&self) -> Result<StoreData, StoreReadError> {
        Ok(self.data.borrow().clone())
    }
}
