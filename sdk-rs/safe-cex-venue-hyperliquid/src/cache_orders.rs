//! Cross-cache order lifecycle and exchange delegation; full classes stay pending.
use crate::{cross_cache::CrossCacheHost, spot_json::JsString};

pub trait CacheOrdersHost: CrossCacheHost {
    fn now(&mut self) -> Result<Self::Value, Self::Error>;
    fn object_keys(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn get_index(
        &mut self,
        value: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Strict ordinary delete, including proxy failures/nonconfigurable properties.
    fn delete(&mut self, value: &Self::Value, key: &Self::Value) -> Result<(), Self::Error>;
    /// Actual subtraction then comparison, preserving native coercion exceptions.
    fn older_than(
        &mut self,
        now: &Self::Value,
        timestamp: &Self::Value,
        milliseconds: f64,
    ) -> Result<bool, Self::Error>;
    /// Source getCrossCache dependency, including construction/errors, host-bound.
    fn singleton(&mut self) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn method<H: CacheOrdersHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
pub fn clear_stale_session_storage<H: CacheOrdersHost>(h: &mut H, cache: &H::Value) {
    let _ignored: Result<(), H::Error> = (|| {
        let storage = method(h, cache, "getSessionStorage", &[])?;
        if h.truthy(&storage) {
            for key in [
                "tealstreet_hip3_seenFills",
                "tealstreet_hip3_prePlacement",
                "tealstreet_hip3_pendingOrders",
                "tealstreet_hl_placementRecords",
            ] {
                let key = h.literal(&text(key));
                method(h, &storage, "removeItem", &[key])?;
            }
        }
        Ok(())
    })();
}
pub fn clear_pre_placement<H: CacheOrdersHost>(
    h: &mut H,
    cache: &H::Value,
    coin: &H::Value,
    side: &H::Value,
) -> Result<(), H::Error> {
    let key = method(
        h,
        cache,
        "getPrePlacementKey",
        &[coin.clone(), side.clone()],
    )?;
    let record = method(h, cache, "readPrePlacementCache", &[])?;
    let entry = h.get_index(&record, &key)?;
    if h.truthy(&entry) {
        h.delete(&record, &key)?;
        method(h, cache, "writePrePlacementCache", &[record])?;
    }
    let map = h.get(cache, "workerPrePlacementCache")?;
    method(h, &map, "delete", &[key.clone()])?;
    let action = h.literal(&text("removePrePlacement"));
    method(h, cache, "broadcastCacheUpdate", &[action, key])?;
    Ok(())
}
pub fn get_pending_order<H: CacheOrdersHost>(
    h: &mut H,
    cache: &H::Value,
    order_id: &H::Value,
) -> Result<H::Value, H::Error> {
    let record = method(h, cache, "readPendingOrdersCache", &[])?;
    // Captured before cleanup: even a stale requested entry is returned once.
    let entry = h.get_index(&record, order_id)?;
    let mut needs_write = false;
    let now = h.now()?;
    let keys = h.object_keys(&record)?;
    let iterator = h.entries(&keys)?;
    while let Some(key) = h.iterator_step(&iterator)? {
        let body = (|| {
            let candidate = h.get_index(&record, &key)?;
            let timestamp = h.get(&candidate, "timestamp")?;
            if h.older_than(&now, &timestamp, 60000.0)? {
                h.delete(&record, &key)?;
                needs_write = true;
            }
            Ok(())
        })();
        if let Err(error) = body {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    if needs_write {
        method(h, cache, "writePendingOrdersCache", &[record])?;
    }
    Ok(entry)
}
pub fn add_pending_order<H: CacheOrdersHost>(
    h: &mut H,
    cache: &H::Value,
    order_id: &H::Value,
    kind: &H::Value,
    price: &H::Value,
) -> Result<(), H::Error> {
    let entry = h.object()?;
    h.define_data(&entry, &text("type"), kind)?;
    h.define_data(&entry, &text("price"), price)?;
    let timestamp = h.now()?;
    h.define_data(&entry, &text("timestamp"), &timestamp)?;
    let record = method(h, cache, "readPendingOrdersCache", &[])?;
    // The source computed property performs ToPropertyKey before assignment.
    let key = h.property_key(order_id)?;
    h.set(&record, &key, &entry)?;
    method(h, cache, "writePendingOrdersCache", &[record])?;
    let map = h.get(cache, "workerPendingOrdersCache")?;
    method(h, &map, "set", &[order_id.clone(), entry.clone()])?;
    let action = h.literal(&text("addPending"));
    method(
        h,
        cache,
        "broadcastCacheUpdate",
        &[action, order_id.clone(), entry],
    )?;
    Ok(())
}
pub fn remove_pending_order<H: CacheOrdersHost>(
    h: &mut H,
    cache: &H::Value,
    order_id: &H::Value,
) -> Result<(), H::Error> {
    let record = method(h, cache, "readPendingOrdersCache", &[])?;
    h.delete(&record, order_id)?;
    method(h, cache, "writePendingOrdersCache", &[record])?;
    let map = h.get(cache, "workerPendingOrdersCache")?;
    method(h, &map, "delete", &[order_id.clone()])?;
    let action = h.literal(&text("removePending"));
    method(
        h,
        cache,
        "broadcastCacheUpdate",
        &[action, order_id.clone()],
    )?;
    Ok(())
}
fn delegate<H: CacheOrdersHost>(
    h: &mut H,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let cache = h.singleton()?;
    method(h, &cache, name, args)
}
pub fn exchange_add_to_pre_placement_cache<H: CacheOrdersHost>(
    h: &mut H,
    coin: &H::Value,
    side: &H::Value,
    kind: &H::Value,
    price: &H::Value,
    amount: &H::Value,
) -> Result<(), H::Error> {
    delegate(
        h,
        "addPrePlacement",
        &[
            coin.clone(),
            side.clone(),
            kind.clone(),
            price.clone(),
            amount.clone(),
        ],
    )?;
    Ok(())
}
pub fn exchange_match_pre_placement<H: CacheOrdersHost>(
    h: &mut H,
    coin: &H::Value,
    side: &H::Value,
    price: Option<&H::Value>,
    amount: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let price = price.cloned().unwrap_or_else(|| h.undefined());
    let amount = amount.cloned().unwrap_or_else(|| h.undefined());
    delegate(
        h,
        "matchPrePlacement",
        &[coin.clone(), side.clone(), price, amount],
    )
}
pub fn exchange_get_pending_hip3_order<H: CacheOrdersHost>(
    h: &mut H,
    order_id: &H::Value,
) -> Result<H::Value, H::Error> {
    delegate(h, "getPendingOrder", &[order_id.clone()])
}
pub fn exchange_add_pending_hip3_order<H: CacheOrdersHost>(
    h: &mut H,
    order_id: &H::Value,
    kind: &H::Value,
    price: &H::Value,
) -> Result<(), H::Error> {
    delegate(
        h,
        "addPendingOrder",
        &[order_id.clone(), kind.clone(), price.clone()],
    )?;
    Ok(())
}
pub fn exchange_remove_pending_hip3_order<H: CacheOrdersHost>(
    h: &mut H,
    order_id: &H::Value,
) -> Result<(), H::Error> {
    delegate(h, "removePendingOrder", &[order_id.clone()])?;
    Ok(())
}
