//! Primary Bybit's synchronous local order lifecycle and reconciliation timers.
//! Maps retain insertion order and live iterator semantics; orders retain mutable identity.
use safe_cex_types::{Order, OrderType};
use serde_json::Value;
use std::{cell::RefCell, collections::HashSet, rc::Rc};
pub type OrderHandle = Rc<RefCell<Order>>;
pub type OrderList = Rc<RefCell<Vec<OrderHandle>>>;
pub type TimerList<T> = Rc<RefCell<Vec<T>>>;
#[derive(Clone, Debug)]
pub struct RemovalOrder {
    pub id: String,
}
pub type RemovalList = Rc<RefCell<Vec<Rc<RefCell<RemovalOrder>>>>>;
pub type TimerCallback<'a, E> = Rc<dyn Fn() -> Result<(), E> + 'a>;
pub type ReconcileMethod<'a, R, E> = Rc<dyn Fn(&str) -> Result<R, E> + 'a>;
pub type ReconcileGetter<'a, R, E> = Rc<dyn Fn() -> Result<ReconcileMethod<'a, R, E>, E> + 'a>;

/// A delete followed by reinsert appends; replacing an existing key retains its slot.
#[derive(Default)]
struct OrderedMap<T> {
    slots: Vec<Option<(String, T)>>,
}
impl<T: Clone> OrderedMap<T> {
    fn get(&self, key: &str) -> Option<T> {
        self.slots
            .iter()
            .flatten()
            .find(|(id, _)| id == key)
            .map(|(_, v)| v.clone())
    }
    fn set(&mut self, key: String, value: T) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(id, _)| id == &key))
        {
            *slot = Some((key, value));
        } else {
            self.slots.push(Some((key, value)));
        }
    }
    fn delete(&mut self, key: &str) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(id, _)| id == key))
        {
            *slot = None;
        }
    }
    fn snapshot(&self) -> Vec<(String, T)> {
        self.slots.iter().flatten().cloned().collect()
    }
}
#[derive(Clone)]
pub struct RecentEntry {
    pub placed_at: f64,
    pub order: OrderHandle,
}
#[derive(Clone, Debug)]
pub struct RetiredSymbol {
    pub symbol: String,
    pub retired_at: f64,
}
pub struct PrimaryOrderState<T> {
    recent: RefCell<OrderedMap<Rc<RefCell<RecentEntry>>>>,
    timers: RefCell<OrderedMap<TimerList<T>>>,
    retired: RefCell<OrderedMap<RetiredSymbol>>,
}
impl<T> Default for PrimaryOrderState<T> {
    fn default() -> Self {
        Self {
            recent: RefCell::new(OrderedMap { slots: vec![] }),
            timers: RefCell::new(OrderedMap { slots: vec![] }),
            retired: RefCell::new(OrderedMap { slots: vec![] }),
        }
    }
}
impl<T: Clone> PrimaryOrderState<T> {
    pub fn recent(&self) -> Vec<(String, Rc<RefCell<RecentEntry>>)> {
        self.recent.borrow().snapshot()
    }
    pub fn timers(&self) -> Vec<(String, TimerList<T>)> {
        self.timers.borrow().snapshot()
    }
    pub fn retired(&self) -> Vec<(String, RetiredSymbol)> {
        self.retired.borrow().snapshot()
    }
    pub fn put_recent(&self, key: String, entry: RecentEntry) {
        self.recent
            .borrow_mut()
            .set(key, Rc::new(RefCell::new(entry)));
    }
    pub fn put_timers(&self, key: String, timers: TimerList<T>) {
        self.timers.borrow_mut().set(key, timers);
    }
    pub fn put_retired(&self, key: String, entry: RetiredSymbol) {
        self.retired.borrow_mut().set(key, entry);
    }
    pub fn delete_recent(&self, key: &str) {
        self.recent.borrow_mut().delete(key);
    }
}
pub fn should_use_local_rest_order_lifecycle_fallback<E>(
    is_demo: impl FnOnce() -> Result<bool, E>,
    site_id: impl FnOnce() -> Result<Option<Value>, E>,
) -> Result<bool, E> {
    if is_demo()? {
        return Ok(true);
    }
    Ok(site_id()?.as_ref().and_then(Value::as_str) == Some(crate::site::BYBIT_BRAZIL_SITE_ID))
}
pub fn mark_order_seen<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    order_id: &str,
    clear: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    state.recent.borrow_mut().delete(order_id);
    clear(order_id)
}
pub fn clear_recently_placed_order_reconciliation_timers<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    order_id: &str,
    mut clear_timeout: impl FnMut(T) -> Result<(), E>,
) -> Result<(), E> {
    let Some(timers) = state.timers.borrow().get(order_id) else {
        return Ok(());
    };
    // Array.forEach captures initial length, but reads each slot at callback time.
    let length = timers.borrow().len();
    for i in 0..length {
        let timer = timers.borrow().get(i).cloned();
        if let Some(timer) = timer {
            clear_timeout(timer)?;
        }
    }
    state.timers.borrow_mut().delete(order_id);
    Ok(())
}
pub fn should_reconcile_local_reduce_only_limit_order<E>(
    order: &OrderHandle,
    fallback: impl FnOnce() -> Result<bool, E>,
) -> Result<bool, E> {
    if !fallback()? {
        return Ok(false);
    }
    let order = order.borrow();
    Ok(order.r#type == OrderType::Limit && order.reduce_only == true)
}
pub fn schedule_local_reduce_only_limit_order_reconciliation<'a, T: Clone, R: 'a, E: 'a>(
    state: &PrimaryOrderState<T>,
    order: OrderHandle,
    should_reconcile: impl FnOnce(&OrderHandle) -> Result<bool, E>,
    clear: impl FnOnce(&str) -> Result<(), E>,
    mut set_timeout: impl FnMut(TimerCallback<'a, E>, f64) -> Result<T, E>,
    get_reconcile: ReconcileGetter<'a, R, E>,
) -> Result<(), E> {
    if !should_reconcile(&order)? {
        return Ok(());
    }
    let id = order.borrow().id.clone();
    clear(&id)?;
    let mut timers = Vec::new();
    for delay in [1000., 3000., 8250.] {
        let order = order.clone();
        let getter = get_reconcile.clone();
        let callback: TimerCallback<'a, E> = Rc::new(move || {
            let method = getter()?;
            let id = order.borrow().id.clone();
            let _discarded = method(&id)?;
            Ok(())
        });
        timers.push(set_timeout(callback, delay)?);
    }
    let id = order.borrow().id.clone();
    state
        .timers
        .borrow_mut()
        .set(id, Rc::new(RefCell::new(timers)));
    Ok(())
}
pub fn get_filled_order_reconciliation_target(order: &OrderHandle) -> f64 {
    let order = order.borrow();
    let filled = if order.filled != 0. && !order.filled.is_nan() {
        order.filled
    } else {
        0.
    };
    let amounts = [order.amount, order.remaining + filled]
        .into_iter()
        .filter(|n| n.is_finite())
        .collect::<Vec<_>>();
    if amounts.is_empty() {
        0.
    } else {
        amounts
            .into_iter()
            .fold(f64::NEG_INFINITY, safe_cex_num::math_max)
    }
}
pub fn record_recently_placed_order<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    order: OrderHandle,
    now: impl FnOnce() -> Result<f64, E>,
    fallback: impl FnOnce() -> Result<bool, E>,
    add: impl FnOnce(OrderList) -> Result<(), E>,
    schedule: impl FnOnce(OrderHandle) -> Result<(), E>,
) -> Result<(), E> {
    if order.borrow().r#type == OrderType::Market {
        return Ok(());
    }
    let id = order.borrow().id.clone();
    let placed_at = now()?;
    state.recent.borrow_mut().set(
        id,
        Rc::new(RefCell::new(RecentEntry {
            placed_at,
            order: order.clone(),
        })),
    );
    if fallback()? {
        add(Rc::new(RefCell::new(vec![order.clone()])))?;
        schedule(order)?;
    }
    Ok(())
}
pub fn retire_cancelled_orders<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    orders: OrderList,
    now: impl FnOnce() -> Result<f64, E>,
    mut mark_seen: impl FnMut(&str) -> Result<(), E>,
    fallback: impl FnOnce() -> Result<bool, E>,
    remove: impl FnOnce(RemovalList) -> Result<(), E>,
) -> Result<(), E> {
    if orders.borrow().is_empty() {
        return Ok(());
    }
    let retired_at = now()?;
    let mut i = 0;
    loop {
        let order = orders.borrow().get(i).cloned();
        let Some(order) = order else {
            break;
        };
        i += 1;
        let id = order.borrow().id.clone();
        mark_seen(&id)?;
        if !order.borrow().symbol.is_empty() {
            let order = order.borrow();
            state.retired.borrow_mut().set(
                order.id.clone(),
                RetiredSymbol {
                    symbol: order.symbol.clone(),
                    retired_at,
                },
            );
        }
    }
    if fallback()? {
        let ids = orders
            .borrow()
            .iter()
            .map(|order| {
                Rc::new(RefCell::new(RemovalOrder {
                    id: order.borrow().id.clone(),
                }))
            })
            .collect();
        remove(Rc::new(RefCell::new(ids)))?;
    }
    Ok(())
}
pub fn resolve_order_symbol_for_fills<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    order_id: &str,
    store_orders: impl FnOnce() -> Result<OrderList, E>,
    now: impl FnOnce() -> Result<f64, E>,
) -> Result<Option<String>, E> {
    let orders = store_orders()?;
    let live = orders
        .borrow()
        .iter()
        .find(|order| order.borrow().id == order_id)
        .map(|order| order.borrow().symbol.clone());
    if live.as_ref().is_some_and(|s| !s.is_empty()) {
        return Ok(live);
    }
    let now = now()?;
    let mut i = 0;
    loop {
        let slot = state.retired.borrow().slots.get(i).cloned();
        let Some(slot) = slot else {
            break;
        };
        i += 1;
        if let Some((id, entry)) = slot {
            if now - entry.retired_at > 60000. {
                state.retired.borrow_mut().delete(&id);
            }
        }
    }
    if let Some(entry) = state.retired.borrow().get(order_id) {
        return Ok(Some(entry.symbol));
    }
    Ok(state
        .recent
        .borrow()
        .get(order_id)
        .map(|entry| entry.borrow().order.borrow().symbol.clone()))
}
pub fn preserve_recently_placed_orders<T: Clone, E>(
    state: &PrimaryOrderState<T>,
    fetched: OrderList,
    now: impl FnOnce() -> Result<f64, E>,
    mut mark_seen: impl FnMut(&str) -> Result<(), E>,
    mut store_orders: impl FnMut() -> Result<OrderList, E>,
) -> Result<OrderList, E> {
    if state.recent.borrow().slots.iter().all(Option::is_none) {
        return Ok(fetched);
    }
    let now = now()?;
    let fetched_ids: HashSet<_> = fetched
        .borrow()
        .iter()
        .map(|order| order.borrow().id.clone())
        .collect();
    let mut preserved = Vec::new();
    let mut i = 0;
    loop {
        let slot = state.recent.borrow().slots.get(i).cloned();
        let Some(slot) = slot else {
            break;
        };
        i += 1;
        let Some((id, entry)) = slot else {
            continue;
        };
        if fetched_ids.contains(&id) || now - entry.borrow().placed_at > 8000. {
            mark_seen(&id)?;
            continue;
        }
        let store = store_orders()?;
        let order = store
            .borrow()
            .iter()
            .find(|order| order.borrow().id == id)
            .cloned()
            .unwrap_or_else(|| entry.borrow().order.clone());
        preserved.push(order);
    }
    if preserved.is_empty() {
        Ok(fetched)
    } else {
        let mut result = fetched.borrow().clone();
        result.extend(preserved);
        Ok(Rc::new(RefCell::new(result)))
    }
}
