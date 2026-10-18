// Native test observation delegates every Store operation to actual DefaultStore.
use indexmap::IndexMap;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{
    PackedTickerDelta, Store, StoreMutationError, StoreUpdate,
    model_patch::{ModelPatch, PatchSource},
};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::*;
use serde_json::{Map, Value};
use std::cell::{Cell, RefCell};
type Subscriber = StoreSubscriber;
#[derive(Debug, Clone, PartialEq)]
pub enum WriteCall {
    Orders(Vec<String>),
    Positions(Vec<String>),
    Status(String, OrderStatus),
}
pub struct ObservedStore {
    pub inner: DefaultStore,
    pub market_reads: Cell<usize>,
    pub position_reads: Cell<usize>,
    pub second_market: RefCell<Option<Market>>,
    pub writes: RefCell<Vec<WriteCall>>,
}
impl ObservedStore {
    pub fn new() -> Self {
        Self {
            inner: DefaultStore::new(),
            market_reads: Cell::new(0),
            position_reads: Cell::new(0),
            second_market: RefCell::new(None),
            writes: RefCell::new(vec![]),
        }
    }
}
impl Default for ObservedStore {
    fn default() -> Self {
        Self::new()
    }
}
impl Store for ObservedStore {
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError> {
        Store::update(&self.inner, changes, from_worker)
    }
    fn reset(&self) -> Result<(), StoreMutationError> {
        Store::reset(&self.inner)
    }
    fn subscribe(&self, callback: Subscriber) -> Box<dyn Fn()> {
        Store::subscribe(&self.inner, callback)
    }
    fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError> {
        Store::set_setting(&self.inner, key, value)
    }
    fn remove_order(&self, id: &str) -> Result<(), StoreMutationError> {
        Store::remove_order(&self.inner, id)
    }
    fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<(), StoreMutationError> {
        self.writes
            .borrow_mut()
            .push(WriteCall::Status(id.into(), status));
        Store::update_order_status(&self.inner, id, status)
    }
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        Store::remove_orders(&self.inner, ids)
    }
    fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        Store::update_orders_status(&self.inner, ids, status)
    }
    fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        Store::mark_orders_terminal(&self.inner, ids, status)
    }
    fn update_order(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        Store::update_order(&self.inner, id, changes)
    }
    fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        Store::update_position(&self.inner, id, changes)
    }
    fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        Store::update_positions(&self.inner, updates)
    }
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        Store::add_or_update_position(&self.inner, position)
    }
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError> {
        self.writes.borrow_mut().push(WriteCall::Positions(
            positions.iter().map(|p| p.id.clone()).collect(),
        ));
        Store::add_or_update_positions(&self.inner, positions)
    }
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        Store::add_or_update_positions_full(&self.inner, positions)
    }
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        Store::add_order(&self.inner, order)
    }
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        Store::add_or_update_order(&self.inner, order)
    }
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.writes.borrow_mut().push(WriteCall::Orders(
            orders.iter().map(|o| o.id.clone()).collect(),
        ));
        Store::add_or_update_orders(&self.inner, orders)
    }
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        Store::add_or_update_orders_full(&self.inner, orders)
    }
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        Store::set_orders_for_symbol(&self.inner, symbol, orders)
    }
    fn update_ticker(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        Store::update_ticker(&self.inner, id, changes)
    }
    fn apply_packed_ticker_deltas(
        &self,
        deltas: Vec<PackedTickerDelta>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        Store::apply_packed_ticker_deltas(&self.inner, deltas, from_worker)
    }
    fn update_market(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        Store::update_market(&self.inner, id, changes)
    }
    fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        Store::update_stark_config(&self.inner, data)
    }
    fn update_zk_link_config(&self, data: Option<Input>) -> Result<(), StoreMutationError> {
        Store::update_zk_link_config(&self.inner, data)
    }
    fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        Store::update_user_data(&self.inner, data)
    }
    fn latency(&self) -> Result<f64, StoreReadError> {
        Store::latency(&self.inner)
    }
    fn balance(&self) -> Result<Balance, StoreReadError> {
        Store::balance(&self.inner)
    }
    fn balance_handle(&self) -> Result<safe_cex_store::BalanceHandle, StoreReadError> {
        Store::balance_handle(&self.inner)
    }
    fn markets(&self) -> Result<Vec<Market>, StoreReadError> {
        Store::markets(&self.inner)
    }
    fn tickers(&self) -> Result<Vec<Ticker>, StoreReadError> {
        Store::tickers(&self.inner)
    }
    fn orders(&self) -> Result<Vec<Order>, StoreReadError> {
        Store::orders(&self.inner)
    }
    fn orders_risk(&self) -> Result<IndexMap<String, Vec<OrderRiskProfile>>, StoreReadError> {
        Store::orders_risk(&self.inner)
    }
    fn positions(&self) -> Result<Vec<Position>, StoreReadError> {
        self.position_reads.set(self.position_reads.get() + 1);
        Store::positions(&self.inner)
    }
    fn displayed_positions(&self) -> Result<Vec<DisplayedPosition>, StoreReadError> {
        Store::displayed_positions(&self.inner)
    }
    fn displayed_margin(&self) -> Result<Option<DisplayedMargin>, StoreReadError> {
        Store::displayed_margin(&self.inner)
    }
    fn active_chasers(&self) -> Result<Vec<ActiveChaser>, StoreReadError> {
        Store::active_chasers(&self.inner)
    }
    fn active_twaps(&self) -> Result<Vec<ActiveTWAP>, StoreReadError> {
        Store::active_twaps(&self.inner)
    }
    fn loaded(&self) -> Result<StoreDataLoaded, StoreReadError> {
        Store::loaded(&self.inner)
    }
    fn update_ids(&self) -> Result<StoreUpdateIds, StoreReadError> {
        Store::update_ids(&self.inner)
    }
    fn options(&self) -> Result<StoreOptions, StoreReadError> {
        Store::options(&self.inner)
    }
    fn message_handler_stats(&self) -> Result<Option<Value>, StoreReadError> {
        Store::message_handler_stats(&self.inner)
    }
    fn stark_config(&self) -> Result<Option<Value>, StoreReadError> {
        Store::stark_config(&self.inner)
    }
    fn zk_link_config(&self) -> Result<Option<Input>, StoreReadError> {
        Store::zk_link_config(&self.inner)
    }
    fn user_data(&self) -> Result<Option<Value>, StoreReadError> {
        Store::user_data(&self.inner)
    }
    fn tickers_by_id(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Store::tickers_by_id(&self.inner)
    }
    fn markets_by_id(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        Store::markets_by_id(&self.inner)
    }
    fn tickers_by_symbol(&self) -> Result<IndexMap<String, Ticker>, StoreReadError> {
        Store::tickers_by_symbol(&self.inner)
    }
    fn markets_by_symbol(&self) -> Result<IndexMap<String, Market>, StoreReadError> {
        self.market_reads.set(self.market_reads.get() + 1);
        if self.market_reads.get() == 2 {
            if let Some(market) = self.second_market.borrow_mut().take() {
                Store::update(
                    &self.inner,
                    StoreUpdate {
                        markets: Some(vec![market]),
                        ..Default::default()
                    },
                    false,
                )
                .expect("history fixture real Store update");
            }
        }
        Store::markets_by_symbol(&self.inner)
    }
    fn get_markets_version(&self) -> f64 {
        Store::get_markets_version(&self.inner)
    }
    fn get_tickers_version(&self) -> f64 {
        Store::get_tickers_version(&self.inner)
    }
    fn fetched_leverage(&self) -> Result<IndexMap<String, FetchedLeverage>, StoreReadError> {
        Store::fetched_leverage(&self.inner)
    }
    fn socket_health(&self) -> Result<Option<Value>, StoreReadError> {
        Store::socket_health(&self.inner)
    }
    fn update_socket_health(&self, data: &Map<String, Value>) -> Result<(), StoreMutationError> {
        Store::update_socket_health(&self.inner, data)
    }
    fn request_stats(&self) -> Result<Option<RequestOutcomeCounts>, StoreReadError> {
        Store::request_stats(&self.inner)
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) -> Result<(), StoreMutationError> {
        Store::record_request_outcome(&self.inner, outcome)
    }
    fn serialize(&self) -> Result<StoreData, StoreReadError> {
        Store::serialize(&self.inner)
    }
    fn native_owner(&self) -> std::rc::Rc<dyn std::any::Any> {
        Store::native_owner(&self.inner)
    }
    fn lookup_ticker_by_symbol(
        &self,
        symbol: &str,
    ) -> Result<std::rc::Rc<dyn safe_cex_store::ticker_lookup::TickerLookup>, StoreReadError> {
        Store::lookup_ticker_by_symbol(&self.inner, symbol)
    }
}
