// Test-only forwarding observer; production DefaultStore remains the backing store.
use indexmap::IndexMap;
use safe_cex_store::notification::StoreMutationError;
use safe_cex_store::{
    contract::*,
    model_patch::{ModelPatch, PatchSource},
};
use safe_cex_types::*;
use serde_json::Value;
use std::{cell::Cell, rc::Rc};
pub(super) struct ReadTrace {
    inner: Rc<safe_cex_exchange::native_store::DefaultStore>,
    pub(super) position_reads: Cell<usize>,
    pub(super) ticker_reads: Cell<usize>,
}
impl safe_cex_store::Store for ReadTrace {
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update(self.inner.as_ref(), changes, from_worker)
    }
    fn reset(&self) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::reset(self.inner.as_ref())
    }
    fn subscribe_native(&self, callback: safe_cex_store::contract::NativeStoreSubscriber) -> Box<dyn Fn()> {
        safe_cex_store::Store::subscribe_native(self.inner.as_ref(), callback)
    }
    fn subscribe(&self, callback: StoreSubscriber) -> Box<dyn Fn()> {
        safe_cex_store::Store::subscribe(self.inner.as_ref(), callback)
    }
    fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::set_setting(self.inner.as_ref(), key, value)
    }
    fn remove_order(&self, id: &str) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::remove_order(self.inner.as_ref(), id)
    }
    fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_order_status(self.inner.as_ref(), id, status)
    }
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::remove_orders(self.inner.as_ref(), ids)
    }
    fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_orders_status(self.inner.as_ref(), ids, status)
    }
    fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::mark_orders_terminal(self.inner.as_ref(), ids, status)
    }
    fn update_order(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_order(self.inner.as_ref(), id, changes)
    }
    fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_position(self.inner.as_ref(), id, changes)
    }
    fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_positions(self.inner.as_ref(), updates)
    }
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_position(self.inner.as_ref(), position)
    }
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_positions(self.inner.as_ref(), positions)
    }
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_positions_full(self.inner.as_ref(), positions)
    }
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_order(self.inner.as_ref(), order)
    }
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_order(self.inner.as_ref(), order)
    }
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_orders(self.inner.as_ref(), orders)
    }
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::add_or_update_orders_full(self.inner.as_ref(), orders)
    }
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::set_orders_for_symbol(self.inner.as_ref(), symbol, orders)
    }
    fn update_ticker(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_ticker(self.inner.as_ref(), id, changes)
    }
    fn apply_packed_ticker_deltas(
        &self,
        deltas: Vec<PackedTickerDelta>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::apply_packed_ticker_deltas(self.inner.as_ref(), deltas, from_worker)
    }
    fn update_market(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_market(self.inner.as_ref(), id, changes)
    }
    fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_stark_config(self.inner.as_ref(), data)
    }
    fn update_zk_link_config(
        &self,
        data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_zk_link_config(self.inner.as_ref(), data)
    }
    fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_user_data(self.inner.as_ref(), data)
    }
    fn latency(&self) -> Result<f64, StoreReadError> {
        safe_cex_store::Store::latency(self.inner.as_ref())
    }
    fn balance(&self) -> Result<Balance, StoreReadError> {
        safe_cex_store::Store::balance(self.inner.as_ref())
    }
    fn balance_handle(&self) -> Result<safe_cex_store::BalanceHandle, StoreReadError> {
        safe_cex_store::Store::balance_handle(self.inner.as_ref())
    }
    fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        safe_cex_store::Store::markets(self.inner.as_ref())
    }
    fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        safe_cex_store::Store::tickers(self.inner.as_ref())
    }
    fn orders(&self) -> Result<Vec<Order>, StoreReadError> {
        safe_cex_store::Store::orders(self.inner.as_ref())
    }
    fn orders_risk(&self) -> Result<IndexMap<String, Vec<OrderRiskProfile>>, StoreReadError> {
        safe_cex_store::Store::orders_risk(self.inner.as_ref())
    }
    fn positions(&self) -> Result<Vec<Position>, StoreReadError> {
        self.position_reads.set(self.position_reads.get() + 1);
        safe_cex_store::Store::positions(self.inner.as_ref())
    }
    fn displayed_positions(&self) -> Result<Vec<DisplayedPosition>, StoreReadError> {
        safe_cex_store::Store::displayed_positions(self.inner.as_ref())
    }
    fn displayed_margin(&self) -> Result<Option<DisplayedMargin>, StoreReadError> {
        safe_cex_store::Store::displayed_margin(self.inner.as_ref())
    }
    fn active_chasers(&self) -> Result<Vec<ActiveChaser>, StoreReadError> {
        safe_cex_store::Store::active_chasers(self.inner.as_ref())
    }
    fn active_twaps(&self) -> Result<Vec<ActiveTWAP>, StoreReadError> {
        safe_cex_store::Store::active_twaps(self.inner.as_ref())
    }
    fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        safe_cex_store::Store::loaded(self.inner.as_ref())
    }
    fn update_ids(&self) -> Result<StoreUpdateIds, StoreReadError> {
        safe_cex_store::Store::update_ids(self.inner.as_ref())
    }
    fn options(&self) -> Result<StoreOptions, StoreReadError> {
        safe_cex_store::Store::options(self.inner.as_ref())
    }
    fn message_handler_stats(&self) -> Result<Option<Value>, StoreReadError> {
        safe_cex_store::Store::message_handler_stats(self.inner.as_ref())
    }
    fn stark_config(&self) -> Result<Option<Value>, StoreReadError> {
        safe_cex_store::Store::stark_config(self.inner.as_ref())
    }
    fn zk_link_config(
        &self,
    ) -> Result<Option<safe_cex_types::retained_json::RetainedJsonInput>, StoreReadError> {
        safe_cex_store::Store::zk_link_config(self.inner.as_ref())
    }
    fn user_data(&self) -> Result<Option<Value>, StoreReadError> {
        safe_cex_store::Store::user_data(self.inner.as_ref())
    }
    fn tickers_by_id(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        safe_cex_store::Store::tickers_by_id(self.inner.as_ref())
    }
    fn markets_by_id(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        safe_cex_store::Store::markets_by_id(self.inner.as_ref())
    }
    fn tickers_by_symbol(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        self.ticker_reads.set(self.ticker_reads.get() + 1);
        safe_cex_store::Store::tickers_by_symbol(self.inner.as_ref())
    }
    fn markets_by_symbol(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        safe_cex_store::Store::markets_by_symbol(self.inner.as_ref())
    }
    fn get_markets_version(&self) -> f64 {
        safe_cex_store::Store::get_markets_version(self.inner.as_ref())
    }
    fn get_tickers_version(&self) -> f64 {
        safe_cex_store::Store::get_tickers_version(self.inner.as_ref())
    }
    fn fetched_leverage(&self) -> Result<IndexMap<String, FetchedLeverage>, StoreReadError> {
        safe_cex_store::Store::fetched_leverage(self.inner.as_ref())
    }
    fn socket_health(&self) -> Result<Option<safe_cex_types::retained_json::RetainedJsonInput>, StoreReadError> {
        safe_cex_store::Store::socket_health(self.inner.as_ref())
    }
    fn update_socket_health(&self, data: &safe_cex_types::retained_json::RetainedJsonInput) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_socket_health(self.inner.as_ref(), data)
    }
    fn request_stats(&self) -> Result<Option<RequestOutcomeCounts>, StoreReadError> {
        safe_cex_store::Store::request_stats(self.inner.as_ref())
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::record_request_outcome(self.inner.as_ref(), outcome)
    }
    fn serialize(&self) -> Result<StoreData, StoreReadError> {
        safe_cex_store::Store::serialize(self.inner.as_ref())
    }
    fn native_owner(&self) -> Rc<dyn std::any::Any> {
        safe_cex_store::Store::native_owner(self.inner.as_ref())
    }
    fn lookup_ticker_by_symbol(
        &self,
        symbol: &str,
    ) -> Result<Rc<dyn safe_cex_store::ticker_lookup::TickerLookup>, StoreReadError> {
        safe_cex_store::Store::lookup_ticker_by_symbol(self.inner.as_ref(), symbol)
    }
}
