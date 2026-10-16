//! Individual cross-context cache methods; full class/construction stays pending.
//! Browser globals, opaque records/collections and their native operations are
//! host bindings. These methods retain source order, catch scopes and aliases.
use crate::{json_boundaries::JsPayloadHost, spot_json::JsString};

pub trait CrossCacheHost: JsPayloadHost {
    fn null(&self) -> Self::Value;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    /// A fresh global identifier lookup; absent sessionStorage returns undefined.
    fn session_storage(&mut self) -> Result<Self::Value, Self::Error>;
    fn literal(&self, text: &JsString) -> Self::Value;
    fn object(&mut self) -> Result<Self::Value, Self::Error>;
    /// Strict ordinary assignment (source class methods), including inherited
    /// setters and TypeError when a property/proxy rejects the assignment.
    fn set(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    /// Object-literal CreateDataProperty; never invokes inherited setters.
    fn define_data(
        &mut self,
        object: &Self::Value,
        key: &JsString,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn call(
        &mut self,
        function: &Self::Value,
        receiver: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn parse_json_value(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn property_key(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn entries(&mut self, map: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// IteratorStep. An error here does not close the outer iterator.
    fn iterator_step(&mut self, iterator: &Self::Value)
    -> Result<Option<Self::Value>, Self::Error>;
    /// Destructure an entry with its own native iterator and IteratorClose.
    fn entry_pair(&mut self, entry: &Self::Value) -> Result<[Self::Value; 2], Self::Error>;
    /// For-of IteratorClose on an abrupt body completion. The original throw wins.
    fn close_after_throw(&mut self, iterator: &Self::Value);
    /// Native [...value], preserving element handles and iterator exceptions.
    fn spread(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn greater_than_500(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn method<H: CrossCacheHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
pub fn get_session_storage<H: CrossCacheHost>(h: &mut H) -> H::Value {
    let result = (|| {
        let first = h.session_storage()?;
        if h.is_undefined(&first) {
            Ok(h.null())
        } else {
            h.session_storage()
        }
    })();
    result.unwrap_or_else(|_| h.null())
}
pub fn broadcast_cache_update<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    action: &JsString,
    key: &JsString,
    data: &H::Value,
) -> Result<(), H::Error> {
    let initial = h.get(cache, "orderCacheBroadcast")?;
    if h.truthy(&initial) {
        let _ignored: Result<(), H::Error> = (|| {
            let channel = h.get(cache, "orderCacheBroadcast")?;
            // Method resolution precedes allocation/evaluation of the argument.
            let function = h.get(&channel, "postMessage")?;
            let payload = h.object()?;
            let action = h.literal(action);
            h.define_data(&payload, &text("action"), &action)?;
            let key = h.literal(key);
            h.define_data(&payload, &text("key"), &key)?;
            h.define_data(&payload, &text("data"), data)?;
            h.call(&function, &channel, &[payload])?;
            Ok(())
        })();
    }
    Ok(())
}
pub fn get_pre_placement_key(coin: &JsString, side: &JsString) -> JsString {
    JsString(
        coin.0
            .iter()
            .copied()
            .chain([b'|' as u16])
            .chain(side.0.iter().copied())
            .collect(),
    )
}
fn read_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    storage_key: &str,
    map_name: &str,
    copy_arrays: bool,
) -> Result<H::Value, H::Error> {
    let storage = method(h, cache, "getSessionStorage", &[])?;
    if h.truthy(&storage) {
        let result: Result<H::Value, H::Error> = (|| {
            let key = h.literal(&text(storage_key));
            let data = method(h, &storage, "getItem", &[key])?;
            if h.truthy(&data) {
                h.parse_json_value(&data)
            } else {
                h.object()
            }
        })();
        return match result {
            Ok(v) => Ok(v),
            Err(_) => h.object(),
        };
    }
    let result = h.object()?;
    let map = h.get(cache, map_name)?;
    let iterator = h.entries(&map)?;
    while let Some(entry) = h.iterator_step(&iterator)? {
        let body = (|| {
            let [key, value] = h.entry_pair(&entry)?;
            // Map key property conversion precedes RHS spread in source assignment.
            let key = h.property_key(&key)?;
            let value = if copy_arrays {
                h.spread(&value)?
            } else {
                value
            };
            h.set(&result, &key, &value)
        })();
        if let Err(error) = body {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    Ok(result)
}
pub fn read_pre_placement_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
) -> Result<H::Value, H::Error> {
    read_cache(
        h,
        cache,
        "tealstreet_hip3_prePlacement",
        "workerPrePlacementCache",
        true,
    )
}
pub fn read_pending_orders_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
) -> Result<H::Value, H::Error> {
    read_cache(
        h,
        cache,
        "tealstreet_hip3_pendingOrders",
        "workerPendingOrdersCache",
        false,
    )
}
fn write_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    value: &H::Value,
    key: &str,
) -> Result<(), H::Error> {
    let storage = method(h, cache, "getSessionStorage", &[])?;
    if h.truthy(&storage) {
        let _ignored: Result<(), H::Error> = (|| {
            let function = h.get(&storage, "setItem")?;
            let key = h.literal(&text(key));
            let value = h
                .json_stringify(value)?
                .map(|s| h.literal(&s))
                .unwrap_or_else(|| h.undefined());
            h.call(&function, &storage, &[key, value])?;
            Ok(())
        })();
    }
    Ok(())
}
pub fn write_pre_placement_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    value: &H::Value,
) -> Result<(), H::Error> {
    write_cache(h, cache, value, "tealstreet_hip3_prePlacement")
}
pub fn write_pending_orders_cache<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    value: &H::Value,
) -> Result<(), H::Error> {
    write_cache(h, cache, value, "tealstreet_hip3_pendingOrders")
}
pub fn has_seen_fill<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    fill_id: &JsString,
) -> Result<H::Value, H::Error> {
    let set = h.get(cache, "workerSeenFillsSet")?;
    let id = h.literal(fill_id);
    method(h, &set, "has", &[id])
}
pub fn mark_fill_seen<H: CrossCacheHost>(
    h: &mut H,
    cache: &H::Value,
    fill_id: &JsString,
) -> Result<(), H::Error> {
    let set = h.get(cache, "workerSeenFillsSet")?;
    let id = h.literal(fill_id);
    method(h, &set, "add", &[id])?;
    let set = h.get(cache, "workerSeenFillsSet")?;
    let size = h.get(&set, "size")?;
    if h.greater_than_500(&size)? {
        let set = h.get(cache, "workerSeenFillsSet")?;
        let iterator = method(h, &set, "values", &[])?;
        // Resolve deletion receiver and method before evaluating iterator.next().value.
        let set = h.get(cache, "workerSeenFillsSet")?;
        let delete = h.get(&set, "delete")?;
        let next = method(h, &iterator, "next", &[])?;
        let first = h.get(&next, "value")?;
        h.call(&delete, &set, &[first])?;
    }
    let action = h.literal(&text("seenFill"));
    let id = h.literal(fill_id);
    method(h, cache, "broadcastCacheUpdate", &[action, id])?;
    Ok(())
}
pub fn dispose<H: CrossCacheHost>(h: &mut H, cache: &H::Value) -> Result<(), H::Error> {
    let initial = h.get(cache, "orderCacheBroadcast")?;
    if h.truthy(&initial) {
        let _ignored: Result<H::Value, H::Error> = (|| {
            let channel = h.get(cache, "orderCacheBroadcast")?;
            method(h, &channel, "close", &[])
        })();
        let null = h.null();
        h.set(cache, &h.literal(&text("orderCacheBroadcast")), &null)?;
    }
    Ok(())
}
