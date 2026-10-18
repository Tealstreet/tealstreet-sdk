//! StoreState port. See DERIVED_PARITY.md for representation boundaries.
use crate::contract::{PackedTickerDelta, Store, StoreUpdate};
use crate::model_patch::{ModelPatch, PatchSource, StorePatchError};
use crate::notification::{StoreMutationError, StoreNotificationError};
use crate::virtual_clock::VirtualClock;
use indexmap::IndexMap;
use safe_cex_types::{
    ActiveChaser, ActiveTWAP, Balance, DisplayedMargin, DisplayedPosition, FetchedLeverage, Market,
    Order, OrderRiskProfile, OrderStatus, Position, StoreData, StoreDataLoaded, StoreOptions,
    StoreUpdateIds, Ticker,
};
use safe_cex_types::{RequestOutcome, RequestOutcomeCounts};
use serde_json::{Value, json};
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};

#[cfg(test)]
type Listener = Arc<dyn Fn(&StoreState, bool) + Send + Sync>;
type ContractListener =
    Arc<dyn Fn(&StoreData, bool) -> Result<(), safe_cex_types::StoreSubscriberError>>;
#[derive(Clone)]
enum Subscription {
    #[cfg(test)]
    Legacy(Listener),
    Contract(ContractListener),
}
#[derive(Default)]
struct Listeners {
    next: u64,
    rows: Vec<(u64, Subscription)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PackedTickerDeltas {
    pub packed: Vec<PackedTickerDelta>,
}

pub const PACKED_TICKER_FIELDS: [&str; 11] = [
    "bid",
    "ask",
    "last",
    "mark",
    "index",
    "percentage",
    "openInterest",
    "fundingRate",
    "fundingTime",
    "volume",
    "quoteVolume",
];
pub static DEFAULT_STORE: LazyLock<StoreData> = LazyLock::new(|| {
    serde_json::from_value(json!({
    "latency":0.0,"balance":{"used":0.0,"free":0.0,"total":0.0,"upnl":0.0},
    "markets":[],"tickers":[],"positions":[],"displayedPositions":[],
    "displayedMargin":{"longExposure":0.0,"shortExposure":0.0,"totalNotional":0.0},
    "ordersRisk":{},"activeChasers":[],"activeTWAPs":[],"orders":[],"messageHandlerStats":{},
    "requestStats":{"ok":0.0,"rateLimit":0.0,"auth":0.0,"network":0.0,"other":0.0},
    "fetchedLeverage":{},"loaded":{"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false},
    "options":{"isHedged":false},"updateIds":{"positions":0.0,"orders":0.0},
    "socketHealth":{"publicHealthy":false,"privateHealthy":false,"publicInitialized":false,"privateInitialized":false}
})).expect("defaultStore shape")
});

struct StoreState {
    data: StoreData,
    // Canonical balance identity. data.balance is the owned notification/snapshot
    // projection; unrelated writes must never replace this retained allocation.
    retained_balance: Arc<Balance>,
    ticker_index_map: IndexMap<String, usize>,
    ticker_view_index: IndexMap<String, usize>,
    ticker_symbol_index: IndexMap<String, usize>,
    ticker_symbol_prototype_index: Option<usize>,
    ticker_id_prototype_index: Option<usize>,
    ticker_id_aliases: Vec<Vec<String>>,
    ticker_symbol_aliases: Vec<Vec<String>>,
    market_index_map: IndexMap<String, usize>,
    market_view_index: IndexMap<String, usize>,
    market_symbol_index: IndexMap<String, usize>,
    market_id_aliases: Vec<Vec<String>>,
    market_symbol_aliases: Vec<Vec<String>>,
    tickers_map: IndexMap<String, Ticker>,
    markets_map: IndexMap<String, Market>,
    tickers_by_symbol_map: IndexMap<String, Ticker>,
    markets_by_symbol_map: IndexMap<String, Market>,
    markets_version: f64,
    tickers_version: f64,
    pending_ticker_deltas: IndexMap<usize, serde_json::Map<String, Value>>,
    deferred_notifications: RefCell<Vec<bool>>,
    #[cfg(test)]
    defer_legacy_notifications: std::cell::Cell<bool>,
    request_stats: Option<RequestOutcomeCounts>,
    socket_health: Value,
    listeners: Arc<Mutex<Listeners>>,
    orders: Vec<Order>,
    order_update_id: f64,
    terminal_order_ids: VecDeque<(String, f64)>,
    /// Host/testing clock injection; native default reads the shared virtualClock.
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
}
impl Default for StoreState {
    fn default() -> Self {
        Self {
            data: DEFAULT_STORE.clone(),
            retained_balance: Arc::new(DEFAULT_STORE.balance.clone()),
            ticker_index_map: IndexMap::new(),
            ticker_view_index: IndexMap::new(),
            ticker_symbol_index: IndexMap::new(),
            ticker_symbol_prototype_index: None,
            ticker_id_prototype_index: None,
            ticker_id_aliases: Vec::new(),
            ticker_symbol_aliases: Vec::new(),
            market_index_map: IndexMap::new(),
            market_view_index: IndexMap::new(),
            market_symbol_index: IndexMap::new(),
            market_id_aliases: Vec::new(),
            market_symbol_aliases: Vec::new(),
            tickers_map: IndexMap::new(),
            markets_map: IndexMap::new(),
            tickers_by_symbol_map: IndexMap::new(),
            markets_by_symbol_map: IndexMap::new(),
            markets_version: 0.0,
            tickers_version: 0.0,
            pending_ticker_deltas: IndexMap::new(),
            deferred_notifications: RefCell::new(Vec::new()),
            #[cfg(test)]
            defer_legacy_notifications: std::cell::Cell::new(false),
            request_stats: Some(empty_request_counts()),
            socket_health: json!({"publicHealthy":false,"privateHealthy":false,"publicInitialized":false,"privateInitialized":false}),
            listeners: Arc::default(),
            orders: Vec::new(),
            order_update_id: 0.0,
            terminal_order_ids: VecDeque::new(),
            now: Arc::new(|| <&'static VirtualClock>::default().get_current_time_ms()),
        }
    }
}
impl StoreState {
    #[cfg(test)]
    fn new() -> Self {
        Self::default()
    }
    fn request_stats(&self) -> Option<&RequestOutcomeCounts> {
        self.request_stats.as_ref()
    }
    fn socket_health(&self) -> &Value {
        &self.socket_health
    }
    /// This hot path must neither allocate nor notify.
    fn record_request_outcome(&mut self, outcome: RequestOutcome) {
        if let Some(stats) = &mut self.request_stats {
            let count = match outcome {
                RequestOutcome::Ok => &mut stats.ok,
                RequestOutcome::RateLimit => &mut stats.rate_limit,
                RequestOutcome::Auth => &mut stats.auth,
                RequestOutcome::Network => &mut stats.network,
                RequestOutcome::Other => &mut stats.other,
            };
            *count += 1.0;
        }
    }
    fn serialize(&self) -> Value {
        serde_json::to_value(self.serializable_snapshot()).expect("finite store model")
    }
    fn full_state(&self) -> StoreData {
        let mut data = self.data.clone();
        data.orders = self.orders.clone();
        data.request_stats = self.request_stats.clone();
        data.socket_health = Some(self.socket_health.clone());
        data.update_ids.orders = self.order_update_id;
        data
    }
    fn serializable_snapshot(&self) -> StoreData {
        let mut data = self.full_state();
        // ts-bug: DefaultStore.serialize omits userData and zkLinkConfig.
        data.user_data = None;
        data.zk_link_config = None;
        if data.displayed_margin.is_none() {
            data.displayed_margin = Some(DisplayedMargin {
                long_exposure: 0.0,
                short_exposure: 0.0,
                total_notional: 0.0,
            });
        }
        data
    }
    fn update_socket_health(&mut self, data: &serde_json::Map<String, Value>) {
        self.socket_health
            .as_object_mut()
            .unwrap()
            .extend(data.clone());
        self.notify(false);
    }
    fn reset(&mut self) {
        let defaults = Self::default();
        self.request_stats = defaults.request_stats;
        self.socket_health = defaults.socket_health;
        self.orders.clear();
        self.order_update_id = 0.0;
        self.terminal_order_ids.clear();
        self.data = DEFAULT_STORE.clone();
        self.retained_balance = defaults.retained_balance;
        self.ticker_index_map.clear();
        self.ticker_view_index.clear();
        self.ticker_symbol_index.clear();
        self.ticker_symbol_prototype_index = None;
        self.ticker_id_prototype_index = None;
        self.ticker_id_aliases.clear();
        self.ticker_symbol_aliases.clear();
        self.market_index_map.clear();
        self.market_view_index.clear();
        self.market_symbol_index.clear();
        self.market_id_aliases.clear();
        self.market_symbol_aliases.clear();
        self.tickers_map.clear();
        self.markets_map.clear();
        self.tickers_by_symbol_map.clear();
        self.markets_by_symbol_map.clear();
        self.pending_ticker_deltas.clear();
        self.markets_version = 0.0;
        self.tickers_version = 0.0;
        self.notify(false);
    }

    fn orders(&self) -> &[Order] {
        &self.orders
    }
    fn is_remembered_terminal(&mut self, id: &str, now: f64) -> bool {
        let Some(index) = self
            .terminal_order_ids
            .iter()
            .position(|(key, _)| key == id)
        else {
            return false;
        };
        if now - self.terminal_order_ids[index].1 < 60_000.0 {
            return true;
        }
        self.terminal_order_ids.remove(index);
        false
    }
    fn keep_terminal_orders_closed(&mut self, incoming: Vec<Order>) -> (Vec<Order>, bool) {
        if self.terminal_order_ids.is_empty() {
            return (incoming, false);
        }
        let now = (self.now)();
        let mut changed = false;
        let mut kept = Vec::with_capacity(incoming.len());
        for order in incoming {
            if !terminal(order.status) && self.is_remembered_terminal(&order.id, now) {
                changed = true;
                if let Some(current) = self.orders.iter().find(|o| o.id == order.id)
                    && terminal(current.status)
                {
                    kept.push(current.clone());
                }
            } else {
                kept.push(order);
            }
        }
        (kept, changed)
    }
    /// Only Closed/Canceled are accepted, representing the narrowed TS signature.
    fn mark_orders_terminal(
        &mut self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), &'static str> {
        if !terminal(status) {
            return Err("terminal status must be closed or canceled");
        }
        let now = (self.now)();
        for id in ids {
            self.terminal_order_ids.retain(|(key, _)| key != id);
            self.terminal_order_ids.push_back(((*id).to_owned(), now));
        }
        while self.terminal_order_ids.len() > 1000 {
            self.terminal_order_ids.pop_front();
        }
        self.update_orders_status(ids, status);
        Ok(())
    }
    fn update_orders_status(&mut self, ids: &[&str], status: OrderStatus) {
        let mut changed = false;
        for order in &mut self.orders {
            if ids.contains(&order.id.as_str()) {
                order.status = (status).into();
                changed = true;
            }
        }
        if changed {
            self.order_update_id += 1.0;
            self.data.update_ids.orders = self.order_update_id;
            self.notify(false);
        }
    }
    fn remove_orders(&mut self, ids: &[&str]) {
        let previous = self.orders.len();
        self.orders.retain(|o| !ids.contains(&o.id.as_str()));
        if self.orders.len() != previous {
            self.order_update_id += 1.0;
            self.data.update_ids.orders = self.order_update_id;
            self.notify(false);
        }
    }
    fn remove_order(&mut self, id: &str) {
        if let Some(index) = self.orders.iter().position(|o| o.id == id) {
            self.orders.remove(index);
            self.order_update_id += 1.0;
            self.data.update_ids.orders = self.order_update_id;
            self.notify(false);
        }
    }
    fn add_order(&mut self, order: Order) {
        let (mut orders, changed) = self.keep_terminal_orders_closed(vec![order]);
        if changed {
            return;
        }
        self.orders.push(orders.remove(0));
        self.order_update_id += 1.0;
        self.data.update_ids.orders = self.order_update_id;
        self.notify(false);
    }
    fn add_or_update_order(&mut self, order: Order) {
        let (mut orders, changed) = self.keep_terminal_orders_closed(vec![order]);
        if changed {
            return;
        }
        let order = orders.remove(0);
        if let Some(index) = self.orders.iter().position(|o| o.id == order.id) {
            merge_order(&mut self.orders[index], order);
        } else {
            self.orders.push(order);
        }
        self.order_update_id += 1.0;
        self.data.update_ids.orders = self.order_update_id;
        self.notify(false);
    }
    fn add_or_update_orders(&mut self, incoming: Vec<Order>) {
        let (orders, _) = self.keep_terminal_orders_closed(incoming);
        if orders.is_empty() {
            return;
        }
        // ts-bug: the TS batch index chooses the LAST duplicate existing ID,
        // while the small-batch linear lookup chooses the FIRST. Keep both.
        let mut index = (orders.len() > 1 && self.orders.len() > 1).then(|| {
            self.orders
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id.clone(), i))
                .collect::<HashMap<_, _>>()
        });
        for order in orders {
            let found = match &index {
                Some(index) => index.get(&order.id).copied(),
                None => self.orders.iter().position(|o| o.id == order.id),
            };
            if let Some(i) = found {
                merge_order(&mut self.orders[i], order);
            } else {
                if let Some(index) = &mut index {
                    index.insert(order.id.clone(), self.orders.len());
                }
                self.orders.push(order);
            }
        }
        self.order_update_id += 1.0;
        self.data.update_ids.orders = self.order_update_id;
        self.notify(false);
    }
    fn add_or_update_orders_full(&mut self, incoming: Vec<Order>) {
        self.orders = self.keep_terminal_orders_closed(incoming).0;
        self.order_update_id += 1.0;
        self.data.update_ids.orders = self.order_update_id;
        self.notify(false);
    }
    fn set_orders_for_symbol(&mut self, symbol: &str, incoming: Vec<Order>) {
        let kept = self.keep_terminal_orders_closed(incoming).0;
        self.orders.retain(|o| o.symbol != symbol);
        self.orders.extend(kept);
        self.order_update_id += 1.0;
        self.data.update_ids.orders = self.order_update_id;
        self.notify(false);
    }
    /// Orders-only projection of update(); other StoreUpdate fields remain pending.
    fn update(
        &mut self,
        orders: Option<Vec<Order>>,
        worker_order_id: Option<f64>,
        from_worker: bool,
    ) {
        let adopt = from_worker && worker_order_id.is_some();
        let mut orders = orders;
        if adopt && worker_order_id == Some(self.order_update_id) {
            orders = None;
        }
        let changed_id = worker_order_id.is_some_and(|id| id != self.order_update_id);
        if orders.is_none() && (worker_order_id.is_none() || (from_worker && !changed_id)) {
            return;
        }
        if let Some(id) = worker_order_id {
            self.order_update_id = id;
            self.data.update_ids.orders = id;
        }
        if let Some(orders) = orders {
            self.orders = if from_worker {
                orders
            } else {
                self.keep_terminal_orders_closed(orders).0
            };
            if !adopt {
                self.order_update_id += 1.0;
                self.data.update_ids.orders = self.order_update_id;
            }
        }
        self.notify(from_worker);
    }
    #[cfg(test)]
    fn subscribe(&self, cb: Listener) -> impl Fn() + 'static {
        let mut listeners = self.listeners.lock().unwrap();
        if !listeners
            .rows
            .iter()
            .any(|(_, old)| matches!(old,Subscription::Legacy(old) if Arc::ptr_eq(old, &cb)))
        {
            let id = listeners.next;
            listeners.next += 1;
            listeners.rows.push((id, Subscription::Legacy(cb.clone())));
        }
        let registry = self.listeners.clone();
        move || {
            registry.lock().unwrap().rows.retain(
                |(_, old)| !matches!(old,Subscription::Legacy(old) if Arc::ptr_eq(old, &cb)),
            )
        }
    }
    fn notify(&self, from_worker: bool) {
        #[cfg(test)]
        if !self.defer_legacy_notifications.get() {
            let mut cursor = None;
            loop {
                let next = self
                    .listeners
                    .lock()
                    .unwrap()
                    .rows
                    .iter()
                    .find(|(id, _)| cursor.is_none_or(|cursor| *id > cursor))
                    .cloned();
                let Some((id, cb)) = next else {
                    break;
                };
                cursor = Some(id);
                match cb {
                    #[cfg(test)]
                    Subscription::Legacy(cb) => cb(self, from_worker),
                    Subscription::Contract(_) => {
                        panic!("contract listeners require the production deferred fanout")
                    }
                }
            }
            return;
        }
        self.deferred_notifications.borrow_mut().push(from_worker);
    }
}
fn empty_request_counts() -> RequestOutcomeCounts {
    RequestOutcomeCounts {
        ok: 0.0,
        rate_limit: 0.0,
        auth: 0.0,
        network: 0.0,
        other: 0.0,
    }
}
fn terminal(status: impl Into<safe_cex_types::ModelField<OrderStatus>>) -> bool {
    let status = status.into();
    matches!(
        status,
        safe_cex_types::ModelField::Value(OrderStatus::Closed | OrderStatus::Canceled)
    )
}
fn merge_order(target: &mut Order, incoming: Order) {
    let old = std::mem::replace(target, incoming);
    if target.parent_id.is_absent() {
        target.parent_id = old.parent_id;
    }
    if target.status.is_absent() {
        target.status = old.status;
    }
    if target.r#type.is_absent() {
        target.r#type = old.r#type;
    }
    if target.side.is_absent() {
        target.side = old.side;
    }
    if target.price.is_absent() {
        target.price = old.price;
    }
    if target.reduce_only.is_absent() {
        target.reduce_only = old.reduce_only;
    }
    if target.close.is_absent() {
        target.close = old.close;
    }
    if target.info.is_absent() {
        target.info = old.info;
    }
    if target.client_order_id.is_absent() {
        target.client_order_id = old.client_order_id;
    }
    if target.time_in_force.is_absent() {
        target.time_in_force = old.time_in_force;
    }
    if target.trigger_type.is_absent() {
        target.trigger_type = old.trigger_type;
    }
    if target.tp_trigger_type.is_absent() {
        target.tp_trigger_type = old.tp_trigger_type;
    }
    if target.sl_trigger_type.is_absent() {
        target.sl_trigger_type = old.sl_trigger_type;
    }
    if target.take_profit.is_absent() {
        target.take_profit = old.take_profit;
    }
    if target.stop_loss.is_absent() {
        target.stop_loss = old.stop_loss;
    }
    if target.extra.is_absent() {
        target.extra = old.extra;
    }
}

impl StoreState {
    fn latency(&self) -> f64 {
        self.data.latency
    }
    fn balance(&self) -> &Balance {
        &self.retained_balance
    }
    fn markets(&self) -> &[Market] {
        &self.data.markets
    }
    fn tickers(&self) -> &[Ticker] {
        &self.data.tickers
    }
    fn orders_risk(&self) -> &IndexMap<String, Vec<OrderRiskProfile>> {
        &self.data.orders_risk
    }
    fn positions(&self) -> &[Position] {
        &self.data.positions
    }
    fn displayed_positions(&self) -> &[DisplayedPosition] {
        &self.data.displayed_positions
    }
    fn displayed_margin(&self) -> Option<&DisplayedMargin> {
        self.data.displayed_margin.as_ref()
    }
    fn active_chasers(&self) -> &[ActiveChaser] {
        &self.data.active_chasers
    }
    fn active_twaps(&self) -> &[ActiveTWAP] {
        &self.data.active_twaps
    }
    fn loaded(&self) -> &StoreDataLoaded {
        &self.data.loaded
    }
    fn options(&self) -> &StoreOptions {
        &self.data.options
    }
    fn update_ids(&self) -> StoreUpdateIds {
        StoreUpdateIds {
            orders: self.order_update_id,
            positions: self.data.update_ids.positions,
        }
    }
    fn message_handler_stats(&self) -> Option<&Value> {
        self.data.message_handler_stats.as_ref()
    }
    fn stark_config(&self) -> Option<&Value> {
        self.data.stark_config.as_ref()
    }
    fn zk_link_config(&self) -> Option<&safe_cex_types::retained_json::RetainedJsonInput> {
        self.data.zk_link_config.as_ref()
    }
    fn user_data(&self) -> Option<&Value> {
        self.data.user_data.as_ref()
    }
    fn fetched_leverage(&self) -> &IndexMap<String, FetchedLeverage> {
        &self.data.fetched_leverage
    }
    fn tickers_by_id(&self) -> &IndexMap<String, Ticker> {
        &self.tickers_map
    }
    fn markets_by_id(&self) -> &IndexMap<String, Market> {
        &self.markets_map
    }
    fn tickers_by_symbol(&self) -> &IndexMap<String, Ticker> {
        &self.tickers_by_symbol_map
    }
    fn markets_by_symbol(&self) -> &IndexMap<String, Market> {
        &self.markets_by_symbol_map
    }
    fn get_markets_version(&self) -> f64 {
        self.markets_version
    }
    fn get_tickers_version(&self) -> f64 {
        self.tickers_version
    }
    fn build_id_index_map(ids: impl Iterator<Item = String>) -> IndexMap<String, usize> {
        ids.enumerate().map(|(i, id)| (id, i)).collect()
    }
    fn build_id_lookup(ids: impl Iterator<Item = String>) -> IndexMap<String, bool> {
        ids.map(|id| (id, true)).collect()
    }
    fn find_index_by_id(
        mut ids: impl Iterator<Item = String>,
        id: &str,
        index: Option<&IndexMap<String, usize>>,
    ) -> Option<usize> {
        match index {
            Some(index) => index.get(id).copied(),
            None => ids.position(|value| value == id),
        }
    }
    fn update_in_array<T: crate::model_patch::PatchModel + Clone>(
        target: &mut T,
        changes: &dyn PatchSource,
    ) -> Result<(), StorePatchError> {
        crate::model_patch::apply(target, changes)
    }
    fn patch_object_in_place(target: &mut Market, incoming: Market) -> bool {
        // The source treats supplied precision/limits records as changed, even with equal values.
        crate::model_patch::merge_market(target, incoming);
        true
    }
    fn strip_ticker_info(mut ticker: Ticker) -> Ticker {
        if ticker
            .info
            .as_ref()
            .is_some_and(safe_cex_types::retained_json::RetainedJsonInput::is_truthy)
        {
            ticker.info = None;
        }
        ticker
    }
    fn strip_ticker_info_from_changes(
        changes: &dyn PatchSource,
        visitor: &mut dyn FnMut(
            &str,
            crate::model_patch::PatchValueRef<'_>,
        ) -> Result<(), StorePatchError>,
    ) -> Result<bool, StorePatchError> {
        let mut any = false;
        changes.visit(&mut |field, value| {
            if field != "info" {
                visitor(field, value)?;
                any = true;
            }
            Ok(())
        })?;
        Ok(any)
    }
    fn patch_ticker_in_place(target: &mut Ticker, source: &Ticker) -> bool {
        let mut changed = false;
        if target.bid != source.bid {
            target.bid = source.bid;
            changed = true;
        }
        if target.ask != source.ask {
            target.ask = source.ask;
            changed = true;
        }
        if target.last != source.last {
            target.last = source.last;
            changed = true;
        }
        if target.mark != source.mark {
            target.mark = source.mark;
            changed = true;
        }
        if target.index != source.index {
            target.index = source.index;
            changed = true;
        }
        if target.percentage != source.percentage {
            target.percentage = source.percentage;
            changed = true;
        }
        if target.open_interest != source.open_interest {
            target.open_interest = source.open_interest;
            changed = true;
        }
        if target.funding_rate != source.funding_rate {
            target.funding_rate = source.funding_rate;
            changed = true;
        }
        if target.funding_time != source.funding_time {
            target.funding_time = source.funding_time;
            changed = true;
        }
        if target.volume != source.volume {
            target.volume = source.volume;
            changed = true;
        }
        if target.quote_volume != source.quote_volume {
            target.quote_volume = source.quote_volume;
            changed = true;
        }
        changed
    }
    fn patch_worker_tickers_in_place(&mut self, tickers: &[Ticker]) -> Option<bool> {
        if tickers.is_empty()
            || tickers.len() != self.data.tickers.len()
            || tickers
                .iter()
                .zip(&self.data.tickers)
                .any(|(a, b)| a.id != b.id || a.symbol != b.symbol)
        {
            return None;
        }
        let mut changed = false;
        for (target, source) in self.data.tickers.iter_mut().zip(tickers) {
            changed = Self::patch_ticker_in_place(target, source) || changed;
        }
        for index in 0..self.data.tickers.len() {
            self.copy_ticker_alias_values(index);
        }
        Some(changed)
    }
    fn patch_worker_markets_in_place(&mut self, markets: Vec<Market>) -> Result<bool, Vec<Market>> {
        if markets.is_empty()
            || markets.len() != self.data.markets.len()
            || markets
                .iter()
                .zip(&self.data.markets)
                .any(|(a, b)| a.id != b.id || a.symbol != b.symbol)
        {
            return Err(markets);
        }
        let mut changed = false;
        for (target, source) in self.data.markets.iter_mut().zip(markets) {
            changed = Self::patch_object_in_place(target, source) || changed;
        }
        for index in 0..self.data.markets.len() {
            self.copy_market_alias_values(index);
        }
        Ok(changed)
    }
    fn refresh_ticker_views(&mut self) {
        self.ticker_id_aliases
            .resize_with(self.data.tickers.len(), Vec::new);
        self.ticker_symbol_aliases
            .resize_with(self.data.tickers.len(), Vec::new);
        for index in 0..self.data.tickers.len() {
            self.sync_ticker_view(index);
        }
    }
    fn sync_ticker_view(&mut self, index: usize) {
        let ticker = &self.data.tickers[index];
        if ticker.symbol == "__proto__" {
            self.ticker_symbol_prototype_index = Some(index);
        }
        if ticker.id == "__proto__" {
            self.ticker_id_prototype_index = Some(index);
        }
        bind_ticker_alias(
            &mut self.ticker_view_index,
            &mut self.ticker_id_aliases,
            &ticker.id,
            index,
        );
        bind_ticker_alias(
            &mut self.ticker_symbol_index,
            &mut self.ticker_symbol_aliases,
            &ticker.symbol,
            index,
        );
        self.copy_ticker_alias_values(index);
    }
    fn copy_ticker_alias_values(&mut self, index: usize) {
        let ticker = &self.data.tickers[index];
        for key in &self.ticker_id_aliases[index] {
            match self.tickers_map.get_mut(key) {
                Some(target) => copy_ticker(target, ticker),
                None => {
                    self.tickers_map.insert(key.clone(), ticker.clone());
                }
            }
        }
        for key in &self.ticker_symbol_aliases[index] {
            match self.tickers_by_symbol_map.get_mut(key) {
                Some(target) => copy_ticker(target, ticker),
                None => {
                    self.tickers_by_symbol_map
                        .insert(key.clone(), ticker.clone());
                }
            }
        }
    }
    fn refresh_market_views(&mut self) {
        self.market_id_aliases
            .resize_with(self.data.markets.len(), Vec::new);
        self.market_symbol_aliases
            .resize_with(self.data.markets.len(), Vec::new);
        for index in 0..self.data.markets.len() {
            self.sync_market_view(index);
        }
    }
    fn sync_market_view(&mut self, index: usize) {
        let market = &self.data.markets[index];
        bind_ticker_alias(
            &mut self.market_view_index,
            &mut self.market_id_aliases,
            &market.id,
            index,
        );
        bind_ticker_alias(
            &mut self.market_symbol_index,
            &mut self.market_symbol_aliases,
            &market.symbol,
            index,
        );
        self.copy_market_alias_values(index);
    }
    fn copy_market_alias_values(&mut self, index: usize) {
        let market = &self.data.markets[index];
        for key in &self.market_id_aliases[index] {
            self.markets_map.insert(key.clone(), market.clone());
        }
        for key in &self.market_symbol_aliases[index] {
            self.markets_by_symbol_map
                .insert(key.clone(), market.clone());
        }
    }
    fn audit_positions(&mut self, remove_dupes: bool) {
        let mut seen = IndexMap::new();
        let positions: Vec<_> = self
            .data
            .positions
            .iter()
            .filter(|p| seen.insert(p.id.clone(), ()).is_none())
            .cloned()
            .collect();
        if positions.len() != self.data.positions.len() {
            eprintln!("Duplicate positions detected...");
            if remove_dupes {
                eprintln!("Removing duplicate positions");
                self.data.positions = positions.clone();
            }
        }
        if positions.iter().any(|p| p.contracts < 0.0) {
            eprintln!("Negative contracts detected");
        }
    }
    fn update_order_status(&mut self, id: &str, status: OrderStatus) {
        if let Some(index) = self.orders.iter().position(|o| o.id == id) {
            self.orders[index].status = (status).into();
            self.order_update_id += 1.0;
            self.data.update_ids.orders = self.order_update_id;
            self.notify(false);
        }
    }
    fn update_order(&mut self, id: &str, changes: &dyn PatchSource) -> Result<(), StorePatchError> {
        if let Some(index) = self.orders.iter().position(|o| o.id == id) {
            Self::update_in_array(&mut self.orders[index], changes)?;
            self.order_update_id += 1.0;
            self.data.update_ids.orders = self.order_update_id;
            self.notify(false);
        }
        Ok(())
    }
    fn update_position(
        &mut self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StorePatchError> {
        if let Some(index) = self.data.positions.iter().position(|o| o.id == id) {
            Self::update_in_array(&mut self.data.positions[index], changes)?;
            self.data.update_ids.positions += 1.0;
            self.notify(false);
        }
        Ok(())
    }
    fn add_or_update_position(&mut self, position: Position) {
        if let Some(index) = self.data.positions.iter().position(|p| p.id == position.id) {
            crate::model_patch::merge_position(&mut self.data.positions[index], position);
        } else {
            self.data.positions.push(position);
        }
        self.audit_positions(false);
        self.data.update_ids.positions += 1.0;
        self.notify(false);
    }
    fn update_positions(
        &mut self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StorePatchError> {
        if updates.is_empty() || self.data.positions.is_empty() {
            return Ok(());
        }
        let index = (updates.len() > 1 && self.data.positions.len() > 1)
            .then(|| Self::build_id_index_map(self.data.positions.iter().map(|p| p.id.clone())));
        let changes: Vec<_> = updates
            .iter()
            .map(|(id, patch)| {
                (
                    id.as_ref().and_then(|id| {
                        Self::find_index_by_id(
                            self.data.positions.iter().map(|p| p.id.clone()),
                            id,
                            index.as_ref(),
                        )
                    }),
                    patch,
                )
            })
            .collect();
        if changes.iter().any(|(index, _)| index.is_some()) {
            let mut candidates = self.data.positions.clone();
            for (index, patch) in changes {
                if let Some(index) = index {
                    Self::update_in_array(&mut candidates[index], patch)?;
                }
            }
            self.data.positions = candidates;
            self.data.update_ids.positions += 1.0;
            self.notify(false);
        }
        Ok(())
    }
    fn add_or_update_positions(&mut self, positions: Vec<Position>) {
        if positions.is_empty() {
            return;
        }
        self.upsert_positions(positions);
        self.audit_positions(false);
        self.data.update_ids.positions += 1.0;
        self.notify(false);
    }
    fn upsert_positions(&mut self, positions: Vec<Position>) {
        let mut index = (positions.len() > 1 && self.data.positions.len() > 1)
            .then(|| Self::build_id_index_map(self.data.positions.iter().map(|p| p.id.clone())));
        for position in positions {
            let found = Self::find_index_by_id(
                self.data.positions.iter().map(|p| p.id.clone()),
                &position.id,
                index.as_ref(),
            );
            if let Some(i) = found {
                crate::model_patch::merge_position(&mut self.data.positions[i], position);
            } else {
                if let Some(index) = &mut index {
                    index.insert(position.id.clone(), self.data.positions.len());
                }
                self.data.positions.push(position);
            }
        }
    }
    fn create_empty_position_from(mut position: Position) -> Position {
        position.entry_price = 0.0;
        position.notional = 0.0;
        position.unrealized_pnl = 0.0;
        position.contracts = 0.0;
        position.liquidation_price = 0.0;
        position.margin_size = (Some(0.0)).into();
        position.info = (None).into();
        position.extra = (None).into();
        position
    }
    fn add_or_update_positions_full(&mut self, positions: Vec<Position>) {
        let ids = Self::build_id_lookup(positions.iter().map(|p| p.id.clone()));
        self.upsert_positions(positions);
        for position in &mut self.data.positions {
            if !ids.contains_key(&position.id) {
                *position = Self::create_empty_position_from(position.clone());
            }
        }
        self.data.update_ids.positions += 1.0;
        self.notify(false);
    }
    fn update_ticker(
        &mut self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StorePatchError> {
        let Some(index) = self.ticker_view_index.get(id).copied() else {
            if let Some(prototype) = self.ticker_id_prototype_index {
                let ticker = &self.data.tickers[prototype];
                if crate::ticker_lookup::prototype_field(ticker, id).is_truthy() {
                    Self::strip_ticker_info_from_changes(changes, &mut |field, _| {
                        if let Some(message) =
                            crate::ticker_lookup::primitive_assignment_error(ticker, id, field)
                        {
                            return Err(StorePatchError::source_type_error(field, message));
                        }
                        Ok(())
                    })?;
                }
            }
            return Ok(());
        };
        let mut candidate = self.data.tickers[index].clone();
        let did_change = Self::strip_ticker_info_from_changes(changes, &mut |field, value| {
            use crate::model_patch::PatchModel;
            candidate.patch_field(field, value)
        })?;
        if !did_change {
            return Ok(());
        }
        self.data.tickers[index] = candidate;
        self.sync_ticker_view(index);
        self.notify(false);
        Ok(())
    }
    fn update_market(
        &mut self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StorePatchError> {
        let Some(index) = self.market_index_map.get(id).copied() else {
            return Ok(());
        };
        Self::update_in_array(&mut self.data.markets[index], changes)?;
        self.sync_market_view(index);
        self.markets_version += 1.0;
        self.notify(false);
        Ok(())
    }
    fn apply_ticker_deltas<P: PatchSource>(
        &mut self,
        deltas: &[(f64, P)],
        from_worker: bool,
    ) -> Result<(), StorePatchError> {
        let mut staged = std::collections::HashMap::new();
        let mut commits = Vec::new();
        for (index, patch) in deltas {
            // The source discards empty/info-only patches before accessing a row.
            if !Self::strip_ticker_info_from_changes(patch, &mut |_, _| Ok(()))? {
                continue;
            }
            if *index <= -1.0 || *index >= self.data.tickers.len() as f64 {
                continue;
            }
            if !index.is_finite() || index.fract() != 0.0 || *index < 0.0 {
                return Err(StorePatchError::invalid_index());
            }
            let index = *index as usize;
            let candidate = staged
                .entry(index)
                .or_insert_with(|| self.data.tickers[index].clone());
            Self::strip_ticker_info_from_changes(patch, &mut |field, value| {
                use crate::model_patch::PatchModel;
                candidate.patch_field(field, value)
            })?;
            // Retain the sequence of successful mutations for alias rebinding.
            commits.push((index, candidate.clone()));
        }
        if !commits.is_empty() {
            for (index, candidate) in commits {
                self.data.tickers[index] = candidate;
                self.sync_ticker_view(index);
            }
            self.notify(from_worker);
        }
        Ok(())
    }
    fn pack_ticker_deltas<P: PatchSource>(
        deltas: &[(f64, P)],
    ) -> Result<PackedTickerDeltas, StorePatchError> {
        let mut packed = Vec::new();
        for (index, changes) in deltas {
            let mut numbers = IndexMap::new();
            changes.visit(&mut |field, value| {
                let value = match value {
                    crate::model_patch::PatchValueRef::Number(value) => Some(value),
                    crate::model_patch::PatchValueRef::Native(value) => {
                        value.downcast_ref::<f64>().copied()
                    }
                    crate::model_patch::PatchValueRef::Json(Value::Number(value)) => {
                        Some(safe_cex_num::number_from_string(&value.to_string()))
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    numbers.insert(field.to_owned(), value);
                }
                Ok(())
            })?;
            let mut mask = 0_u32;
            let mut values = Vec::new();
            for (bit, field) in PACKED_TICKER_FIELDS.iter().enumerate() {
                if let Some(value) = numbers.get(*field) {
                    mask |= 1 << bit;
                    values.push(*value);
                }
            }
            if mask != 0 {
                packed.push(PackedTickerDelta {
                    index: *index,
                    mask: f64::from(mask),
                    values,
                });
            }
        }
        Ok(PackedTickerDeltas { packed })
    }
    fn consume_ticker_deltas(&mut self) -> Vec<(f64, serde_json::Map<String, Value>)> {
        std::mem::take(&mut self.pending_ticker_deltas)
            .into_iter()
            .map(|(index, changes)| (index as f64, changes))
            .collect()
    }
    fn apply_packed_ticker_deltas_in_place(&mut self, deltas: &[PackedTickerDelta]) -> bool {
        let mut changed = false;
        for delta in deltas {
            if delta.index < 0.0
                || delta.index >= self.data.tickers.len() as f64
                || delta.mask == 0.0
            {
                continue;
            }
            let mask = js_uint32(delta.mask);
            if mask & ((1 << PACKED_TICKER_FIELDS.len()) - 1) == 0 {
                continue;
            }
            let index = checked_index(delta.index, self.data.tickers.len());
            let mut cursor = 0;
            for (bit, field) in PACKED_TICKER_FIELDS.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    let value = *delta
                        .values
                        .get(cursor)
                        .expect("packed delta has a value for each set bit");
                    cursor += 1;
                    set_ticker_number(&mut self.data.tickers[index], field, value);
                    changed = true;
                }
            }
            self.copy_ticker_alias_values(index);
        }
        changed
    }
    fn apply_packed_ticker_deltas(&mut self, deltas: &[PackedTickerDelta], from_worker: bool) {
        if self.apply_packed_ticker_deltas_in_place(deltas) {
            self.notify(from_worker);
        }
    }
    fn update_stark_config(&mut self, data: Option<Value>) {
        self.data.stark_config = data;
        self.notify(false);
    }
    fn update_zk_link_config(
        &mut self,
        data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) {
        self.data.zk_link_config = data;
        self.notify(false);
    }
    fn update_user_data(&mut self, data: Option<Value>) {
        self.data.user_data = data;
        self.notify(false);
    }
    fn set_setting(&mut self, key: &str, value: bool) {
        assert_eq!(key, "isHedged", "StoreOptions has only isHedged");
        if self.data.options.is_hedged != value {
            self.data.options.is_hedged = value;
            self.notify(false);
        }
    }
    fn update_full(&mut self, mut changes: StoreUpdate, from_worker: bool) {
        let mut changed = false;
        let packed = changes.ticker_deltas.take();
        if packed.is_some() {
            changes.tickers = None;
        }
        if let Some(tickers) = changes.tickers.take() {
            if from_worker && let Some(patched) = self.patch_worker_tickers_in_place(&tickers) {
                changed |= patched;
            } else {
                changes.tickers = Some(tickers.into_iter().map(Self::strip_ticker_info).collect());
            }
        }
        if from_worker && let Some(markets) = changes.markets.take() {
            match self.patch_worker_markets_in_place(markets) {
                Ok(patched) => changed |= patched,
                Err(markets) => changes.markets = Some(markets),
            }
        }
        let adopt_orders = from_worker
            && changes
                .update_ids
                .as_ref()
                .is_some_and(|v| v.orders.is_some());
        let adopt_positions = from_worker
            && changes
                .update_ids
                .as_ref()
                .is_some_and(|v| v.positions.is_some());
        if let Some(ids) = &mut changes.update_ids {
            if adopt_orders && ids.orders == Some(self.order_update_id) {
                changes.orders = None;
            }
            if adopt_positions && ids.positions == Some(self.data.update_ids.positions) {
                changes.positions = None;
            }
            let orders = ids.orders.unwrap_or(self.order_update_id);
            let positions = ids.positions.unwrap_or(self.data.update_ids.positions);
            if from_worker
                && orders == self.order_update_id
                && positions == self.data.update_ids.positions
            {
                changes.update_ids = None;
            } else {
                self.order_update_id = orders;
                self.data.update_ids.orders = orders;
                self.data.update_ids.positions = positions;
            }
        }
        if let Some(packed) = packed {
            changed |= self.apply_packed_ticker_deltas_in_place(&packed);
        }
        changed |= [
            changes.latency.is_some(),
            changes.balance.is_some(),
            changes.markets.is_some(),
            changes.tickers.is_some(),
            changes.orders.is_some(),
            changes.stark_config.is_some(),
            changes.zk_link_config.is_some(),
            changes.user_data.is_some(),
            changes.positions.is_some(),
            changes.displayed_positions.is_some(),
            changes.displayed_margin.is_some(),
            changes.orders_risk.is_some(),
            changes.active_chasers.is_some(),
            changes.active_twaps.is_some(),
            changes.loaded.is_some(),
            changes.options.is_some(),
            changes.update_ids.is_some(),
            changes.message_handler_stats.is_some(),
            changes.request_stats.is_some(),
            changes.fetched_leverage.is_some(),
            changes.socket_health.is_some(),
            changes.ticker_deltas.is_some(),
        ]
        .into_iter()
        .any(|present| present);
        if !changed {
            return;
        }
        if let Some(orders) = changes.orders.take() {
            self.orders = if from_worker {
                orders
            } else {
                self.keep_terminal_orders_closed(orders).0
            };
            if !adopt_orders {
                self.order_update_id += 1.0;
                self.data.update_ids.orders = self.order_update_id;
            }
        }
        if let Some(positions) = changes.positions.take() {
            self.data.positions = positions;
            self.audit_positions(true);
            if !adopt_positions {
                self.data.update_ids.positions += 1.0;
            }
        }
        if let Some(tickers) = changes.tickers.take() {
            self.data.tickers = tickers;
            self.tickers_version += 1.0;
            self.pending_ticker_deltas.clear();
            self.ticker_index_map =
                Self::build_id_index_map(self.data.tickers.iter().map(|t| t.id.clone()));
            self.tickers_map.clear();
            self.tickers_by_symbol_map.clear();
            self.ticker_view_index.clear();
            self.ticker_symbol_index.clear();
            self.ticker_symbol_prototype_index = None;
            self.ticker_id_prototype_index = None;
            self.ticker_id_aliases.clear();
            self.ticker_symbol_aliases.clear();
            self.refresh_ticker_views();
        }
        if let Some(markets) = changes.markets.take() {
            self.data.markets = markets;
            self.markets_version += 1.0;
            self.market_index_map =
                Self::build_id_index_map(self.data.markets.iter().map(|m| m.id.clone()));
            self.markets_map.clear();
            self.markets_by_symbol_map.clear();
            self.market_view_index.clear();
            self.market_symbol_index.clear();
            self.market_id_aliases.clear();
            self.market_symbol_aliases.clear();
            self.refresh_market_views();
        }
        // Replace the object, rather than mutate an allocation retained by a
        // prior fallback read. Both native views are synchronized before notify.
        if let Some(balance) = changes.balance.take() {
            self.retained_balance = Arc::new(balance.clone());
            self.data.balance = balance;
        }
        if let Some(value) = changes.latency {
            self.data.latency = value;
        }
        if let Some(value) = changes.displayed_positions {
            self.data.displayed_positions = value;
        }
        if let Some(value) = changes.orders_risk {
            self.data.orders_risk = value;
        }
        if let Some(value) = changes.active_chasers {
            self.data.active_chasers = value;
        }
        if let Some(value) = changes.active_twaps {
            self.data.active_twaps = value;
        }
        if let Some(value) = changes.loaded {
            self.data.loaded = value;
        }
        if let Some(value) = changes.options {
            self.data.options = value;
        }
        if let Some(value) = changes.fetched_leverage {
            self.data.fetched_leverage = value;
        }
        if let Some(value) = changes.displayed_margin {
            self.data.displayed_margin = Some(value);
        }
        if let Some(value) = changes.message_handler_stats {
            self.data.message_handler_stats = Some(value);
        }
        if let Some(value) = changes.stark_config {
            self.data.stark_config = Some(value);
        }
        if let Some(value) = changes.zk_link_config {
            self.data.zk_link_config = Some(value);
        }
        if let Some(value) = changes.user_data {
            self.data.user_data = Some(value);
        }
        if let Some(stats) = changes.request_stats {
            self.request_stats = Some(stats);
        }
        if let Some(health) = changes.socket_health {
            self.socket_health = health;
        }
        self.notify(from_worker);
    }
}
fn bind_ticker_alias(
    indexes: &mut IndexMap<String, usize>,
    aliases: &mut [Vec<String>],
    key: &str,
    index: usize,
) {
    if indexes.get(key) == Some(&index) {
        return;
    }
    if let Some(old) = indexes.insert(key.to_owned(), index) {
        aliases[old].retain(|alias| alias != key);
    }
    aliases[index].push(key.to_owned());
}
fn copy_ticker(target: &mut Ticker, source: &Ticker) {
    // Aliases mirror the same JS object. Copy numeric bits even when JS equality
    // would consider the previous value equal (notably +0 and -0).
    target.bid = source.bid;
    target.ask = source.ask;
    target.last = source.last;
    target.mark = source.mark;
    target.index = source.index;
    target.percentage = source.percentage;
    target.open_interest = source.open_interest;
    target.funding_rate = source.funding_rate;
    target.funding_time = source.funding_time;
    target.volume = source.volume;
    target.quote_volume = source.quote_volume;
    target.mid_price = source.mid_price;
    target.timestamp = source.timestamp;
    if target.id != source.id {
        target.id.clone_from(&source.id);
    }
    if target.symbol != source.symbol {
        target.symbol.clone_from(&source.symbol);
    }
    if target.info != source.info {
        target.info.clone_from(&source.info);
    }
}

fn checked_index(index: f64, len: usize) -> usize {
    assert!(
        index.is_finite() && index.fract() == 0.0 && index >= 0.0 && index < len as f64,
        "ticker delta index must address a row"
    );
    index as usize
}
fn js_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        0
    } else {
        value.trunc().rem_euclid(4294967296.0) as u32
    }
}
fn set_ticker_number(ticker: &mut Ticker, field: &str, value: f64) {
    match field {
        "bid" => ticker.bid = value,
        "ask" => ticker.ask = value,
        "last" => ticker.last = value,
        "mark" => ticker.mark = value,
        "index" => ticker.index = value,
        "percentage" => ticker.percentage = value,
        "openInterest" => ticker.open_interest = value,
        "fundingRate" => ticker.funding_rate = value,
        "fundingTime" => ticker.funding_time = value,
        "volume" => ticker.volume = value,
        "quoteVolume" => ticker.quote_volume = value,
        "midPrice" => ticker.mid_price = Some(value),
        _ => {}
    }
}

fn shared_mutate<T>(
    store: &RefCell<StoreState>,
    operation: impl FnOnce(&mut StoreState) -> T,
) -> Result<T, StoreNotificationError> {
    let (events, result) = {
        let mut value = store.borrow_mut();
        #[cfg(test)]
        value.defer_legacy_notifications.set(true);
        let result = operation(&mut value);
        value.data.update_ids.orders = value.order_update_id;
        (
            std::mem::take(&mut *value.deferred_notifications.borrow_mut()),
            result,
        )
    };
    for from_worker in events {
        shared_notify(store, from_worker)?;
    }
    Ok(result)
}
fn shared_notify(
    store: &RefCell<StoreState>,
    from_worker: bool,
) -> Result<(), StoreNotificationError> {
    let registry = store.borrow().listeners.clone();
    let mut cursor = None;
    loop {
        let next = registry
            .lock()
            .unwrap()
            .rows
            .iter()
            .find(|(id, _)| cursor.is_none_or(|cursor| *id > cursor))
            .cloned();
        let Some((id, callback)) = next else {
            break;
        };
        cursor = Some(id);
        match callback {
            Subscription::Contract(callback) => {
                let snapshot = store.borrow().full_state();
                callback(&snapshot, from_worker)
                    .map_err(|error| StoreNotificationError::from_subscriber(id, error))?;
            }
            #[cfg(test)]
            Subscription::Legacy(callback) => callback(&store.borrow(), from_worker),
        }
    }
    Ok(())
}
/// Complete production Store, including the interior mutability required by shared Rc callers.
pub struct DefaultStore {
    state: RefCell<StoreState>,
    /// Host/testing clock injection; defaults to virtualClock.
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
}
impl Default for DefaultStore {
    fn default() -> Self {
        let state = StoreState::default();
        Self {
            now: state.now.clone(),
            state: RefCell::new(state),
        }
    }
}
impl DefaultStore {
    pub fn new() -> Self {
        Self::default()
    }
    fn mutate<T>(
        &self,
        operation: impl FnOnce(&mut StoreState) -> T,
    ) -> Result<T, StoreNotificationError> {
        self.state.borrow_mut().now = self.now.clone();
        shared_mutate(&self.state, operation)
    }
    pub fn subscribe(&self, callback: ContractListener) -> Box<dyn Fn()> {
        let registry = self.state.borrow().listeners.clone();
        {
            let mut listeners = registry.lock().unwrap();
            if !listeners.rows.iter().any(
                |(_, old)| matches!(old,Subscription::Contract(old) if Arc::ptr_eq(old,&callback)),
            ) {
                let id = listeners.next;
                listeners.next += 1;
                listeners
                    .rows
                    .push((id, Subscription::Contract(callback.clone())));
            }
        }
        Box::new(move || {
            registry.lock().unwrap().rows.retain(
                |(_, old)| !matches!(old,Subscription::Contract(old) if Arc::ptr_eq(old,&callback)),
            )
        })
    }
    pub fn request_stats(&self) -> Option<Ref<'_, RequestOutcomeCounts>> {
        Ref::filter_map(self.state.borrow(), |state| state.request_stats()).ok()
    }
    pub fn socket_health(&self) -> Ref<'_, Value> {
        Ref::map(self.state.borrow(), |state| state.socket_health())
    }
    pub fn record_request_outcome(&self, outcome: RequestOutcome) {
        self.state.borrow_mut().record_request_outcome(outcome)
    }
    pub fn serialize(&self) -> Value {
        self.state.borrow().serialize()
    }
    pub fn update_socket_health(
        &self,
        data: &serde_json::Map<String, Value>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_socket_health(data))
            .map_err(StoreMutationError::from)
    }
    pub fn reset(&self) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.reset())
            .map_err(StoreMutationError::from)
    }
    pub fn orders(&self) -> Ref<'_, [Order]> {
        Ref::map(self.state.borrow(), |state| state.orders())
    }
    pub fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.mark_orders_terminal(ids, status))?
            .map_err(StoreMutationError::from)
    }
    pub fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_orders_status(ids, status))
            .map_err(StoreMutationError::from)
    }
    pub fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.remove_orders(ids))
            .map_err(StoreMutationError::from)
    }
    pub fn remove_order(&self, id: &str) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.remove_order(id))
            .map_err(StoreMutationError::from)
    }
    pub fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_order(order))
            .map_err(StoreMutationError::from)
    }
    pub fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_order(order))
            .map_err(StoreMutationError::from)
    }
    pub fn add_or_update_orders(&self, incoming: Vec<Order>) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_orders(incoming))
            .map_err(StoreMutationError::from)
    }
    pub fn add_or_update_orders_full(
        &self,
        incoming: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_orders_full(incoming))
            .map_err(StoreMutationError::from)
    }
    pub fn set_orders_for_symbol(
        &self,
        symbol: &str,
        incoming: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.set_orders_for_symbol(symbol, incoming))
            .map_err(StoreMutationError::from)
    }
    pub fn update(
        &self,
        orders: Option<Vec<Order>>,
        worker_order_id: Option<f64>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update(orders, worker_order_id, from_worker))
            .map_err(StoreMutationError::from)
    }
    pub fn latency(&self) -> f64 {
        self.state.borrow().latency()
    }
    pub fn balance(&self) -> Ref<'_, Balance> {
        Ref::map(self.state.borrow(), |state| state.balance())
    }
    pub fn markets(&self) -> Ref<'_, [Market]> {
        Ref::map(self.state.borrow(), |state| state.markets())
    }
    pub fn tickers(&self) -> Ref<'_, [Ticker]> {
        Ref::map(self.state.borrow(), |state| state.tickers())
    }
    pub fn orders_risk(&self) -> Ref<'_, IndexMap<String, Vec<OrderRiskProfile>>> {
        Ref::map(self.state.borrow(), |state| state.orders_risk())
    }
    pub fn positions(&self) -> Ref<'_, [Position]> {
        Ref::map(self.state.borrow(), |state| state.positions())
    }
    pub fn displayed_positions(&self) -> Ref<'_, [DisplayedPosition]> {
        Ref::map(self.state.borrow(), |state| state.displayed_positions())
    }
    pub fn displayed_margin(&self) -> Option<Ref<'_, DisplayedMargin>> {
        Ref::filter_map(self.state.borrow(), |state| state.displayed_margin()).ok()
    }
    pub fn active_chasers(&self) -> Ref<'_, [ActiveChaser]> {
        Ref::map(self.state.borrow(), |state| state.active_chasers())
    }
    pub fn active_twaps(&self) -> Ref<'_, [ActiveTWAP]> {
        Ref::map(self.state.borrow(), |state| state.active_twaps())
    }
    pub fn loaded(&self) -> Ref<'_, StoreDataLoaded> {
        Ref::map(self.state.borrow(), |state| state.loaded())
    }
    pub fn options(&self) -> Ref<'_, StoreOptions> {
        Ref::map(self.state.borrow(), |state| state.options())
    }
    pub fn update_ids(&self) -> StoreUpdateIds {
        self.state.borrow().update_ids()
    }
    pub fn message_handler_stats(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |state| state.message_handler_stats()).ok()
    }
    pub fn stark_config(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |state| state.stark_config()).ok()
    }
    pub fn zk_link_config(
        &self,
    ) -> Option<Ref<'_, safe_cex_types::retained_json::RetainedJsonInput>> {
        Ref::filter_map(self.state.borrow(), |state| state.zk_link_config()).ok()
    }
    pub fn user_data(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |state| state.user_data()).ok()
    }
    pub fn fetched_leverage(&self) -> Ref<'_, IndexMap<String, FetchedLeverage>> {
        Ref::map(self.state.borrow(), |state| state.fetched_leverage())
    }
    pub fn tickers_by_id(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        Ref::map(self.state.borrow(), |state| state.tickers_by_id())
    }
    pub fn markets_by_id(&self) -> Ref<'_, IndexMap<String, Market>> {
        Ref::map(self.state.borrow(), |state| state.markets_by_id())
    }
    pub fn tickers_by_symbol(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        Ref::map(self.state.borrow(), |state| state.tickers_by_symbol())
    }
    pub fn lookup_ticker_by_symbol(
        &self,
        symbol: &str,
    ) -> crate::ticker_lookup::TickerSymbolLookup {
        let state = self.state.borrow();
        if symbol != "__proto__" {
            if let Some(ticker) = state.tickers_by_symbol_map.get(symbol) {
                return crate::ticker_lookup::TickerSymbolLookup::OwnTicker(ticker.id.clone());
            }
        }
        if let Some(index) = state.ticker_symbol_prototype_index {
            return crate::ticker_lookup::prototype_field(&state.data.tickers[index], symbol);
        }
        crate::ticker_lookup::stock(symbol)
    }

    pub fn markets_by_symbol(&self) -> Ref<'_, IndexMap<String, Market>> {
        Ref::map(self.state.borrow(), |state| state.markets_by_symbol())
    }
    pub fn get_markets_version(&self) -> f64 {
        self.state.borrow().get_markets_version()
    }
    pub fn get_tickers_version(&self) -> f64 {
        self.state.borrow().get_tickers_version()
    }
    pub fn audit_positions(&self, remove_dupes: bool) {
        self.state.borrow_mut().audit_positions(remove_dupes)
    }
    pub fn update_order_status(
        &self,
        id: &str,
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_order_status(id, status))
            .map_err(StoreMutationError::from)
    }
    pub fn update_order(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_order(id, changes))?
            .map_err(StoreMutationError::from)
    }
    pub fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_position(id, changes))?
            .map_err(StoreMutationError::from)
    }
    pub fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_position(position))
            .map_err(StoreMutationError::from)
    }
    pub fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_positions(updates))?
            .map_err(StoreMutationError::from)
    }
    pub fn add_or_update_positions(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_positions(positions))
            .map_err(StoreMutationError::from)
    }
    pub fn create_empty_position_from(position: Position) -> Position {
        StoreState::create_empty_position_from(position)
    }
    pub fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.add_or_update_positions_full(positions))
            .map_err(StoreMutationError::from)
    }
    pub fn update_ticker(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_ticker(id, changes))?
            .map_err(StoreMutationError::from)
    }
    pub fn update_market(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_market(id, changes))?
            .map_err(StoreMutationError::from)
    }
    pub fn apply_ticker_deltas<P: PatchSource>(
        &self,
        deltas: &[(f64, P)],
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.apply_ticker_deltas(deltas, from_worker))?
            .map_err(StoreMutationError::from)
    }
    pub fn pack_ticker_deltas<P: PatchSource>(
        deltas: &[(f64, P)],
    ) -> Result<PackedTickerDeltas, StorePatchError> {
        StoreState::pack_ticker_deltas(deltas)
    }
    pub fn consume_ticker_deltas(&self) -> Vec<(f64, serde_json::Map<String, Value>)> {
        self.state.borrow_mut().consume_ticker_deltas()
    }
    pub fn apply_packed_ticker_deltas(
        &self,
        deltas: &[PackedTickerDelta],
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.apply_packed_ticker_deltas(deltas, from_worker))
            .map_err(StoreMutationError::from)
    }
    pub fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_stark_config(data))
            .map_err(StoreMutationError::from)
    }
    pub fn update_zk_link_config(
        &self,
        data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_zk_link_config(data))
            .map_err(StoreMutationError::from)
    }
    pub fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_user_data(data))
            .map_err(StoreMutationError::from)
    }
    pub fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.set_setting(key, value))
            .map_err(StoreMutationError::from)
    }
    pub fn update_full(
        &self,
        changes: StoreUpdate,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|state| state.update_full(changes, from_worker))
            .map_err(StoreMutationError::from)
    }
}
impl safe_cex_types::PublicDataStore for DefaultStore {
    fn subscribe(&self, callback: safe_cex_types::PublicDataSubscriber) -> Box<dyn Fn()> {
        Store::subscribe(self, Box::new(move |data, _| callback(data)))
    }
    fn tickers(&self) -> Ref<'_, [Ticker]> {
        Store::tickers(self)
    }
    fn markets(&self) -> Ref<'_, [Market]> {
        Store::markets(self)
    }
    fn loaded(&self) -> Ref<'_, StoreDataLoaded> {
        Store::loaded(self)
    }
}

impl Store for DefaultStore {
    fn update(&self, changes: StoreUpdate, from_worker: bool) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_full(changes, from_worker))
            .map_err(StoreMutationError::from)
    }
    fn subscribe(&self, callback: safe_cex_types::StoreSubscriber) -> Box<dyn Fn()> {
        self.subscribe(Arc::from(callback))
    }
    fn reset(&self) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.reset()).map_err(StoreMutationError::from)
    }
    fn set_setting(&self, key: &str, value: bool) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.set_setting(key, value))
            .map_err(StoreMutationError::from)
    }
    fn remove_order(&self, id: &str) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.remove_order(id))
            .map_err(StoreMutationError::from)
    }
    fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_order_status(id, status))
            .map_err(StoreMutationError::from)
    }
    fn remove_orders(&self, ids: &[&str]) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.remove_orders(ids))
            .map_err(StoreMutationError::from)
    }
    fn update_orders_status(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_orders_status(ids, status))
            .map_err(StoreMutationError::from)
    }
    fn mark_orders_terminal(
        &self,
        ids: &[&str],
        status: OrderStatus,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.mark_orders_terminal(ids, status))?
            .map_err(StoreMutationError::from)
    }
    fn update_order(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_order(id, changes))?
            .map_err(StoreMutationError::from)
    }
    fn update_position(
        &self,
        id: &str,
        changes: &dyn PatchSource,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_position(id, changes))?
            .map_err(StoreMutationError::from)
    }
    fn update_positions(
        &self,
        updates: &[(Option<String>, ModelPatch)],
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_positions(updates))?
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_position(&self, position: Position) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_position(position))
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_positions(&self, positions: Vec<Position>) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_positions(positions))
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_positions_full(
        &self,
        positions: Vec<Position>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_positions_full(positions))
            .map_err(StoreMutationError::from)
    }
    fn add_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_order(order))
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_order(&self, order: Order) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_order(order))
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_orders(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_orders(orders))
            .map_err(StoreMutationError::from)
    }
    fn add_or_update_orders_full(&self, orders: Vec<Order>) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.add_or_update_orders_full(orders))
            .map_err(StoreMutationError::from)
    }
    fn set_orders_for_symbol(
        &self,
        symbol: &str,
        orders: Vec<Order>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.set_orders_for_symbol(symbol, orders))
            .map_err(StoreMutationError::from)
    }
    fn update_ticker(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_ticker(id, changes))?
            .map_err(StoreMutationError::from)
    }
    fn apply_packed_ticker_deltas(
        &self,
        deltas: Vec<PackedTickerDelta>,
        from_worker: bool,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.apply_packed_ticker_deltas(&deltas, from_worker))
            .map_err(StoreMutationError::from)
    }
    fn update_market(&self, id: &str, changes: &dyn PatchSource) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_market(id, changes))?
            .map_err(StoreMutationError::from)
    }
    fn update_stark_config(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_stark_config(data))
            .map_err(StoreMutationError::from)
    }
    fn update_zk_link_config(
        &self,
        data: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_zk_link_config(data))
            .map_err(StoreMutationError::from)
    }
    fn update_user_data(&self, data: Option<Value>) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_user_data(data))
            .map_err(StoreMutationError::from)
    }
    fn update_socket_health(
        &self,
        data: &serde_json::Map<String, Value>,
    ) -> Result<(), StoreMutationError> {
        self.mutate(|s| s.update_socket_health(data))
            .map_err(StoreMutationError::from)
    }
    fn record_request_outcome(&self, outcome: RequestOutcome) {
        self.state.borrow_mut().record_request_outcome(outcome);
    }
    fn balance(&self) -> Ref<'_, Balance> {
        Ref::map(self.state.borrow(), |s| s.balance())
    }
    fn balance_handle(&self) -> Arc<Balance> {
        self.state.borrow().retained_balance.clone()
    }
    fn markets(&self) -> Ref<'_, [Market]> {
        Ref::map(self.state.borrow(), |s| s.data.markets.as_slice())
    }
    fn tickers(&self) -> Ref<'_, [Ticker]> {
        Ref::map(self.state.borrow(), |s| s.data.tickers.as_slice())
    }
    fn orders(&self) -> Ref<'_, [Order]> {
        Ref::map(self.state.borrow(), |s| s.orders.as_slice())
    }
    fn orders_risk(&self) -> Ref<'_, IndexMap<String, Vec<OrderRiskProfile>>> {
        Ref::map(self.state.borrow(), |s| &s.data.orders_risk)
    }
    fn positions(&self) -> Ref<'_, [Position]> {
        Ref::map(self.state.borrow(), |s| s.data.positions.as_slice())
    }
    fn displayed_positions(&self) -> Ref<'_, [DisplayedPosition]> {
        Ref::map(self.state.borrow(), |s| {
            s.data.displayed_positions.as_slice()
        })
    }
    fn active_chasers(&self) -> Ref<'_, [ActiveChaser]> {
        Ref::map(self.state.borrow(), |s| s.data.active_chasers.as_slice())
    }
    fn active_twaps(&self) -> Ref<'_, [ActiveTWAP]> {
        Ref::map(self.state.borrow(), |s| s.data.active_twaps.as_slice())
    }
    fn loaded(&self) -> Ref<'_, StoreDataLoaded> {
        Ref::map(self.state.borrow(), |s| &s.data.loaded)
    }
    fn update_ids(&self) -> Ref<'_, StoreUpdateIds> {
        Ref::map(self.state.borrow(), |s| &s.data.update_ids)
    }
    fn options(&self) -> Ref<'_, StoreOptions> {
        Ref::map(self.state.borrow(), |s| &s.data.options)
    }
    fn tickers_by_id(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        Ref::map(self.state.borrow(), |s| &s.tickers_map)
    }
    fn markets_by_id(&self) -> Ref<'_, IndexMap<String, Market>> {
        Ref::map(self.state.borrow(), |s| &s.markets_map)
    }
    fn tickers_by_symbol(&self) -> Ref<'_, IndexMap<String, Ticker>> {
        Ref::map(self.state.borrow(), |s| &s.tickers_by_symbol_map)
    }
    fn lookup_ticker_by_symbol(&self, symbol: &str) -> crate::ticker_lookup::TickerSymbolLookup {
        DefaultStore::lookup_ticker_by_symbol(self, symbol)
    }

    fn markets_by_symbol(&self) -> Ref<'_, IndexMap<String, Market>> {
        Ref::map(self.state.borrow(), |s| &s.markets_by_symbol_map)
    }
    fn fetched_leverage(&self) -> Ref<'_, IndexMap<String, FetchedLeverage>> {
        Ref::map(self.state.borrow(), |s| &s.data.fetched_leverage)
    }
    fn displayed_margin(&self) -> Option<Ref<'_, DisplayedMargin>> {
        Ref::filter_map(self.state.borrow(), |s| s.data.displayed_margin.as_ref()).ok()
    }
    fn message_handler_stats(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |s| {
            s.data.message_handler_stats.as_ref()
        })
        .ok()
    }
    fn stark_config(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |s| s.data.stark_config.as_ref()).ok()
    }
    fn zk_link_config(&self) -> Option<Ref<'_, safe_cex_types::retained_json::RetainedJsonInput>> {
        Ref::filter_map(self.state.borrow(), |s| s.data.zk_link_config.as_ref()).ok()
    }
    fn user_data(&self) -> Option<Ref<'_, Value>> {
        Ref::filter_map(self.state.borrow(), |s| s.data.user_data.as_ref()).ok()
    }
    fn request_stats(&self) -> Option<Ref<'_, RequestOutcomeCounts>> {
        Ref::filter_map(self.state.borrow(), |s| s.request_stats.as_ref()).ok()
    }
    fn socket_health(&self) -> Option<Ref<'_, Value>> {
        Some(Ref::map(self.state.borrow(), |s| &s.socket_health))
    }
    fn latency(&self) -> f64 {
        self.state.borrow().data.latency
    }
    fn get_markets_version(&self) -> f64 {
        self.state.borrow().markets_version
    }
    fn get_tickers_version(&self) -> f64 {
        self.state.borrow().tickers_version
    }
    fn serialize(&self) -> StoreData {
        self.state.borrow().serializable_snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn frozen_typescript_terminal_oracle() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/terminal_oracle.json")).unwrap();
        let mut store = DefaultStore::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        for case in cases {
            let now = case["now"].as_f64().unwrap();
            store.now = Arc::new(move || now);
            let args = &case["args"];
            let ids = |value: &Value| -> Vec<String> {
                value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["id"].as_str().unwrap().to_owned())
                    .collect()
            };
            match case["op"].as_str().unwrap() {
                "addOrder" => store
                    .add_order(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrder" => store
                    .add_or_update_order(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrders" => store
                    .add_or_update_orders(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrdersFull" => store
                    .add_or_update_orders_full(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "setOrdersForSymbol" => store
                    .set_orders_for_symbol(
                        args[0].as_str().unwrap(),
                        serde_json::from_value(args[1].clone()).unwrap(),
                    )
                    .unwrap(),
                "markOrdersTerminal" | "updateOrdersStatus" => {
                    let ids = ids(&args[0]);
                    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
                    let status = serde_json::from_value(args[1].clone()).unwrap();
                    if case["op"] == "markOrdersTerminal" {
                        store.mark_orders_terminal(&refs, status).unwrap();
                    } else {
                        store.update_orders_status(&refs, status).unwrap();
                    }
                }
                "removeOrders" => {
                    let ids = ids(&args[0]);
                    store
                        .remove_orders(&ids.iter().map(String::as_str).collect::<Vec<_>>())
                        .unwrap();
                }
                "removeOrder" => store.remove_order(args[0]["id"].as_str().unwrap()).unwrap(),
                "update" => store
                    .update(
                        args[0]
                            .get("orders")
                            .map(|v| serde_json::from_value(v.clone()).unwrap()),
                        args[0]["updateIds"]["orders"].as_f64(),
                        args[1].as_bool().unwrap(),
                    )
                    .unwrap(),
                "reset" => store.reset().unwrap(),
                op => panic!("unknown {op}"),
            }
            let snapshot = store.serialize();
            let projection = json!({"orders":snapshot["orders"],"updateIds":snapshot["updateIds"],"requestStats":snapshot["requestStats"],"socketHealth":snapshot["socketHealth"]});
            crate::test_support::assert_json_eq(&projection, &case["expected"]);
            assert_eq!(
                calls.load(Ordering::Relaxed) as u64,
                case["notifications"].as_u64().unwrap(),
                "{case}"
            );
        }
    }
    #[test]
    fn terminal_cap_refresh_expiry_and_invalid_status() {
        let mut store = StoreState::new();
        store.now = Arc::new(|| 10.0);
        store
            .mark_orders_terminal(&["old"], OrderStatus::Closed)
            .unwrap();
        for i in 0..999 {
            store
                .mark_orders_terminal(&[&i.to_string()], OrderStatus::Canceled)
                .unwrap();
        }
        store
            .mark_orders_terminal(&["old"], OrderStatus::Closed)
            .unwrap();
        store
            .mark_orders_terminal(&["new"], OrderStatus::Closed)
            .unwrap();
        assert_eq!(store.terminal_order_ids.len(), 1000);
        assert!(!store.is_remembered_terminal("0", 11.0));
        assert!(store.is_remembered_terminal("old", 60009.0));
        assert!(!store.is_remembered_terminal("old", 60010.0));
        assert!(!store.is_remembered_terminal("new", f64::NAN));
        assert!(
            store
                .mark_orders_terminal(&["bad"], OrderStatus::Open)
                .is_err()
        );
        store.reset();
        assert!(store.terminal_order_ids.is_empty());
    }
    #[test]
    fn live_listener_identity_and_old_unsubscribe() {
        let mut store = StoreState::new();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let late: Listener = Arc::new(move |_, _| seen.lock().unwrap().push(2));
        let late_copy = late.clone();
        let seen = calls.clone();
        let first: Listener = Arc::new(move |store, _| {
            seen.lock().unwrap().push(1);
            let _remove = store.subscribe(late_copy.clone());
        });
        let remove = store.subscribe(first.clone());
        let _duplicate = store.subscribe(first.clone());
        store.reset();
        assert_eq!(*calls.lock().unwrap(), vec![1, 2]);
        remove();
        let _new_remove = store.subscribe(first);
        remove(); // The old closure removes the re-added callback identity too.
        calls.lock().unwrap().clear();
        store.reset();
        assert_eq!(*calls.lock().unwrap(), vec![2]);
    }
    #[test]
    fn request_increment_never_notifies_or_changes_socket_health() {
        let mut store = StoreState::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
        }));
        let socket = store.socket_health().clone();
        for _ in 0..1000 {
            store.record_request_outcome(RequestOutcome::Ok);
        }
        assert_eq!(store.request_stats().unwrap().ok, 1000.0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(store.socket_health(), &socket);
        let snapshot = store.serialize();
        store.record_request_outcome(RequestOutcome::Auth);
        assert_eq!(snapshot["requestStats"]["auth"], 0.0);
        store.reset();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(store.request_stats().unwrap(), &empty_request_counts());
        store.request_stats = None;
        store.record_request_outcome(RequestOutcome::Ok);
        assert!(store.serialize().get("requestStats").is_none());
    }
}
