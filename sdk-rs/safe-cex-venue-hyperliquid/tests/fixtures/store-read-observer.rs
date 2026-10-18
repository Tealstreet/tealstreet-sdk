// Test-only forwarding observer; production DefaultStore remains the backing store.
use indexmap::IndexMap;
use safe_cex_store::notification::StoreMutationError;
use safe_cex_store::{
    contract::*,
    model_patch::{ModelPatch, PatchSource},
};
use safe_cex_types::*;
use serde_json::{Map, Value};
use std::{
    cell::{Cell, Ref},
    rc::Rc,
};
pub(super) struct ReadTrace {
    inner: Rc<safe_cex_store::store::DefaultStore>,
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
    fn latency(&self) -> f64 {
        safe_cex_store::Store::latency(self.inner.as_ref())
    }
    fn balance(&self) -> Ref<'_, Balance> {
        safe_cex_store::Store::balance(self.inner.as_ref())
    }
    fn balance_handle(&self) -> std::sync::Arc<Balance> {
        safe_cex_store::Store::balance_handle(self.inner.as_ref())
    }
    fn markets(&self) -> Ref<'_, [Market]> {
        safe_cex_store::Store::markets(self.inner.as_ref())
    }
    fn tickers(&self) -> Ref<'_, [Ticker]> {
        safe_cex_store::Store::tickers(self.inner.as_ref())
    }
    fn orders(&self) -> Ref<'_, [Order]> {
        safe_cex_store::Store::orders(self.inner.as_ref())
    }
    fn orders_risk(&self) -> Ref<'_, IndexMap<String, Vec<OrderRiskProfile>>> {
        safe_cex_store::Store::orders_risk(self.inner.as_ref())
    }
    fn positions(&self) -> Ref<'_, [Position]> {
        self.position_reads.set(self.position_reads.get() + 1);
        safe_cex_store::Store::positions(self.inner.as_ref())
    }
    fn displayed_positions(&self) -> Ref<'_, [DisplayedPosition]> {
        safe_cex_store::Store::displayed_positions(self.inner.as_ref())
    }
    fn displayed_margin(&self) -> Option<Ref<'_, DisplayedMargin>> {
        safe_cex_store::Store::displayed_margin(self.inner.as_ref())
    }
    fn active_chasers(&self) -> Ref<'_, [ActiveChaser]> {
        safe_cex_store::Store::active_chasers(self.inner.as_ref())
    }
    fn active_twaps(&self) -> Ref<'_, [ActiveTWAP]> {
        safe_cex_store::Store::active_twaps(self.inner.as_ref())
    }
    fn loaded(&self) -> Ref<'_, StoreDataLoaded> {
        safe_cex_store::Store::loaded(self.inner.as_ref())
    }
    fn update_ids(&self) -> Ref<'_, StoreUpdateIds> {
        safe_cex_store::Store::update_ids(self.inner.as_ref())
    }
    fn options(&self) -> Ref<'_, StoreOptions> {
        safe_cex_store::Store::options(self.inner.as_ref())
    }
    fn message_handler_stats(&self) -> Option<Ref<'_, Value>> {
        safe_cex_store::Store::message_handler_stats(self.inner.as_ref())
    }
    fn stark_config(&self) -> Option<Ref<'_, Value>> {
        safe_cex_store::Store::stark_config(self.inner.as_ref())
    }
    fn zk_link_config(&self) -> Option<Ref<'_, safe_cex_types::retained_json::RetainedJsonInput>> {
        safe_cex_store::Store::zk_link_config(self.inner.as_ref())
    }
    fn user_data(&self) -> Option<Ref<'_, Value>> {
        safe_cex_store::Store::user_data(self.inner.as_ref())
    }
    fn tickers_by_id(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        safe_cex_store::Store::tickers_by_id(self.inner.as_ref())
    }
    fn markets_by_id(&self) -> Ref<'_, IndexMap<String, Market>> {
        safe_cex_store::Store::markets_by_id(self.inner.as_ref())
    }
    fn tickers_by_symbol(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        self.ticker_reads.set(self.ticker_reads.get() + 1);
        safe_cex_store::Store::tickers_by_symbol(self.inner.as_ref())
    }
    fn markets_by_symbol(&self) -> Ref<'_, IndexMap<String, Market>> {
        safe_cex_store::Store::markets_by_symbol(self.inner.as_ref())
    }
    fn get_markets_version(&self) -> f64 {
        safe_cex_store::Store::get_markets_version(self.inner.as_ref())
    }
    fn get_tickers_version(&self) -> f64 {
        safe_cex_store::Store::get_tickers_version(self.inner.as_ref())
    }
    fn fetched_leverage(&self) -> Ref<'_, IndexMap<String, FetchedLeverage>> {
        safe_cex_store::Store::fetched_leverage(self.inner.as_ref())
    }
    fn socket_health(&self) -> Option<Ref<'_, Value>> {
        safe_cex_store::Store::socket_health(self.inner.as_ref())
    }
    fn update_socket_health(&self, data: &Map<String, Value>) -> Result<(), StoreMutationError> {
        safe_cex_store::Store::update_socket_health(self.inner.as_ref(), data)
    }
    fn request_stats(&self) -> Option<Ref<'_, RequestOutcomeCounts>> {
        safe_cex_store::Store::request_stats(self.inner.as_ref())
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) {
        safe_cex_store::Store::record_request_outcome(self.inner.as_ref(), outcome)
    }
    fn serialize(&self) -> StoreData {
        safe_cex_store::Store::serialize(self.inner.as_ref())
    }
}
