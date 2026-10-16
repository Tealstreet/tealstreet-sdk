//! Identical inverse/spot recent-order lifecycle bodies. The linear exchange
//! additionally owns reconciliation timers and is deliberately not mapped here.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Order, OrderType};
use serde_json::Value;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

/// Ordered Map storage for the anonymous `recentlyPlacedOrders` field.
#[derive(Default)]
pub struct RecentOrderCache {
    entries: RefCell<Vec<(String, f64, Rc<RefCell<Order>>)>>,
}
impl RecentOrderCache {
    /// Read-only snapshot of keys, clock values and retained order references.
    pub fn snapshot(&self) -> Vec<(String, f64, Rc<RefCell<Order>>)> {
        self.entries.borrow().clone()
    }
}

pub fn should_use_local_rest_order_lifecycle_fallback(site_id: Option<&Value>) -> bool {
    site_id.and_then(Value::as_str) == Some(crate::site::BYBIT_BRAZIL_SITE_ID)
}

pub fn mark_order_seen(cache: &RecentOrderCache, order_id: &str) {
    cache
        .entries
        .borrow_mut()
        .retain(|(id, _, _)| id != order_id);
}

pub fn record_recently_placed_order(
    cache: &RecentOrderCache,
    order: Rc<RefCell<Order>>,
    site_id: impl FnOnce() -> Option<Value>,
    now: impl FnOnce() -> f64,
    add_or_update_orders: impl FnOnce(Vec<Rc<RefCell<Order>>>) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if order.borrow().r#type == OrderType::Market {
        return Ok(());
    }
    // JavaScript evaluates the Map.set key before evaluating the value's clock.
    let id = order.borrow().id.clone();
    let placed_at = now();
    {
        let mut entries = cache.entries.borrow_mut();
        if let Some(entry) = entries.iter_mut().find(|(key, _, _)| key == &id) {
            *entry = (id, placed_at, Rc::clone(&order));
        } else {
            entries.push((id, placed_at, Rc::clone(&order)));
        }
    }
    if should_use_local_rest_order_lifecycle_fallback(site_id().as_ref()) {
        // Release the cache borrow before notifying the store: its listener can
        // synchronously mark this order seen, or fail after the cache insertion.
        add_or_update_orders(vec![order])?;
    }
    Ok(())
}

pub fn retire_cancelled_orders(
    cache: &RecentOrderCache,
    order_ids: &[String],
    site_id: impl FnOnce() -> Option<Value>,
    remove_orders: impl FnOnce(&[String]) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if order_ids.is_empty() {
        return Ok(());
    }
    for id in order_ids {
        mark_order_seen(cache, id);
    }
    if should_use_local_rest_order_lifecycle_fallback(site_id().as_ref()) {
        remove_orders(order_ids)?;
    }
    Ok(())
}

pub fn preserve_recently_placed_orders(
    cache: &RecentOrderCache,
    fetched: Vec<Rc<RefCell<Order>>>,
    mut store_orders: impl FnMut() -> Vec<Rc<RefCell<Order>>>,
    now: impl FnOnce() -> f64,
) -> Vec<Rc<RefCell<Order>>> {
    if cache.entries.borrow().is_empty() {
        return fetched;
    }
    let now = now();
    let fetched_ids: HashSet<_> = fetched
        .iter()
        .map(|order| order.borrow().id.clone())
        .collect();
    let mut preserved = Vec::new();
    let mut index = 0;
    loop {
        let entry = cache.entries.borrow().get(index).cloned();
        let Some((id, placed_at, order)) = entry else {
            break;
        };
        if fetched_ids.contains(&id) {
            cache.entries.borrow_mut().remove(index);
            continue;
        }
        if now - placed_at > 8_000. {
            cache.entries.borrow_mut().remove(index);
            continue;
        }
        // Store field reads may inspect the cache. No RefCell borrow crosses
        // this callback; mutation of the cache by an executable field getter
        // remains outside the ordinary-field accessor ABI.
        preserved.push(
            store_orders()
                .iter()
                .find(|order| order.borrow().id == id)
                .map(Rc::clone)
                .unwrap_or_else(|| Rc::clone(&order)),
        );
        index += 1;
    }
    if preserved.is_empty() {
        fetched
    } else {
        let mut result = fetched;
        result.extend(preserved);
        result
    }
}
