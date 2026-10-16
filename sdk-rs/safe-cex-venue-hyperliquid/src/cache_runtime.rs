//! Cross-cache construction, receive callbacks, matching and module singleton.
use crate::{cache_orders::CacheOrdersHost, cross_cache, spot_json::JsString};
use std::cell::RefCell;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheMethod {
    Initialize,
    ClearStorage,
    GetStorage,
    Broadcast,
    PreKey,
    ReadPre,
    WritePre,
    AddPre,
    MatchPre,
    ClearPre,
    ReadPending,
    WritePending,
    GetPending,
    AddPending,
    RemovePending,
    HasFill,
    MarkFill,
    Dispose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    Method(CacheMethod),
    Receive,
    AddValid,
    MatchValid,
    PriceMatch,
    AmountMatch,
}
pub const METHODS: &[(&str, CacheMethod)] = &[
    ("initializeBroadcastChannel", CacheMethod::Initialize),
    ("clearStaleSessionStorage", CacheMethod::ClearStorage),
    ("getSessionStorage", CacheMethod::GetStorage),
    ("broadcastCacheUpdate", CacheMethod::Broadcast),
    ("getPrePlacementKey", CacheMethod::PreKey),
    ("readPrePlacementCache", CacheMethod::ReadPre),
    ("writePrePlacementCache", CacheMethod::WritePre),
    ("addPrePlacement", CacheMethod::AddPre),
    ("matchPrePlacement", CacheMethod::MatchPre),
    ("clearPrePlacement", CacheMethod::ClearPre),
    ("readPendingOrdersCache", CacheMethod::ReadPending),
    ("writePendingOrdersCache", CacheMethod::WritePending),
    ("getPendingOrder", CacheMethod::GetPending),
    ("addPendingOrder", CacheMethod::AddPending),
    ("removePendingOrder", CacheMethod::RemovePending),
    ("hasSeenFill", CacheMethod::HasFill),
    ("markFillSeen", CacheMethod::MarkFill),
    ("dispose", CacheMethod::Dispose),
];
/// Native functions dispatch run_callback; prototype methods use dynamic this,
/// receive/predicate callbacks retain their capture. Browser intrinsics are bound.
pub trait CacheRuntimeHost: CacheOrdersHost + Sized {
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn subtract(&mut self, a: &Self::Value, b: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn less(&mut self, a: &Self::Value, b: &Self::Value) -> Result<bool, Self::Error>;
    fn less_equal(&mut self, a: &Self::Value, b: &Self::Value) -> Result<bool, Self::Error>;
    fn abstract_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn array(&mut self, values: &[Self::Value]) -> Result<Self::Value, Self::Error>;
    /// Native class constructor/prototype pair; constructor binds new_cache.
    fn class_definition(&mut self) -> Result<(Self::Value, Self::Value), Self::Error>;
    /// Class own method: writable/configurable, non-enumerable.
    fn define_method(
        &mut self,
        prototype: &Self::Value,
        name: &JsString,
        function: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn instance(&mut self, prototype: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn new_map(&mut self) -> Result<Self::Value, Self::Error>;
    fn new_set(&mut self) -> Result<Self::Value, Self::Error>;
    fn function(
        &mut self,
        capture: Self::Value,
        body: Callback,
    ) -> Result<Self::Value, Self::Error>;
    fn broadcast_constructor(&mut self) -> Result<Self::Value, Self::Error>;
    fn construct_broadcast(
        &mut self,
        constructor: &Self::Value,
        name: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn destructure(
        &mut self,
        value: &Self::Value,
        keys: &[&str],
    ) -> Result<Vec<Self::Value>, Self::Error>;
}
/// One source module owns the shared class prototype and its singleton slot.
pub struct CacheModule<V> {
    pub constructor: V,
    pub prototype: V,
    singleton: RefCell<Option<V>>,
}
/// Actual instance fields live on the opaque native object/collection handles.
pub struct HyperliquidCrossCache<V> {
    pub value: V,
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn method<H: CacheRuntimeHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    let args = args(h)?;
    h.call(&function, receiver, &args)
}
fn field<H: CacheRuntimeHost>(
    h: &mut H,
    owner: &H::Value,
    name: &str,
    value: &H::Value,
) -> Result<(), H::Error> {
    let key = h.literal(&text(name));
    h.set(owner, &key, value)
}
pub fn new_module<H: CacheRuntimeHost>(h: &mut H) -> Result<CacheModule<H::Value>, H::Error> {
    let (constructor, prototype) = h.class_definition()?;
    h.define_method(&prototype, &text("constructor"), &constructor)?;
    for (name, body) in METHODS {
        let capture = h.undefined();
        let value = h.function(capture, Callback::Method(*body))?;
        h.define_method(&prototype, &text(name), &value)?;
    }
    Ok(CacheModule {
        constructor,
        prototype,
        singleton: RefCell::new(None),
    })
}
/// ts: HyperliquidCrossCache.constructor.
pub fn new_cache<H: CacheRuntimeHost>(
    h: &mut H,
    module: &CacheModule<H::Value>,
) -> Result<HyperliquidCrossCache<H::Value>, H::Error> {
    let owner = h.instance(&module.prototype)?;
    let pending = h.new_map()?;
    h.define_data(&owner, &text("workerPendingOrdersCache"), &pending)?;
    let pre = h.new_map()?;
    h.define_data(&owner, &text("workerPrePlacementCache"), &pre)?;
    let seen = h.new_set()?;
    h.define_data(&owner, &text("workerSeenFillsSet"), &seen)?;
    let null = h.null();
    h.define_data(&owner, &text("orderCacheBroadcast"), &null)?;
    method(h, &owner, "initializeBroadcastChannel", |_| Ok(vec![]))?;
    method(h, &owner, "clearStaleSessionStorage", |_| Ok(vec![]))?;
    Ok(HyperliquidCrossCache { value: owner })
}
/// ts: HyperliquidCrossCache#initializeBroadcastChannel.
pub fn initialize_broadcast_channel<H: CacheRuntimeHost>(h: &mut H, cache: &H::Value) {
    let _caught: Result<(), H::Error> = (|| {
        let first = h.broadcast_constructor()?;
        if !h.is_undefined(&first) {
            let constructor = h.broadcast_constructor()?;
            let name = h.literal(&text("tealstreet_hip3_order_cache"));
            let channel = h.construct_broadcast(&constructor, &name)?;
            field(h, cache, "orderCacheBroadcast", &channel)?;
            let channel = h.get(cache, "orderCacheBroadcast")?;
            let handler = h.function(cache.clone(), Callback::Receive)?;
            field(h, &channel, "onmessage", &handler)?;
        }
        Ok(())
    })();
}
/// Captured onmessage body; asynchronous receives are outside initialization's catch.
pub fn receive<H: CacheRuntimeHost>(
    h: &mut H,
    cache: &H::Value,
    event: &H::Value,
) -> Result<(), H::Error> {
    let data = h.get(event, "data")?;
    let values = h.destructure(&data, &["action", "key", "data"])?;
    let action = &values[0];
    let key = &values[1];
    let data = &values[2];
    let is = |h: &H, s: &str| {
        h.string_units(action)
            .is_some_and(|v| v.iter().copied().eq(s.encode_utf16()))
    };
    if is(h, "addPending") && h.truthy(key) && h.truthy(data) {
        let map = h.get(cache, "workerPendingOrdersCache")?;
        method(h, &map, "set", |_| Ok(vec![key.clone(), data.clone()]))?;
    } else if is(h, "removePending") && h.truthy(key) {
        let map = h.get(cache, "workerPendingOrdersCache")?;
        method(h, &map, "delete", |_| Ok(vec![key.clone()]))?;
    } else if is(h, "addPrePlacement") && h.truthy(key) && h.truthy(data) {
        let map = h.get(cache, "workerPrePlacementCache")?;
        method(h, &map, "set", |h| {
            let data = if h.is_array(data)? {
                data.clone()
            } else {
                h.array(&[data.clone()])?
            };
            Ok(vec![key.clone(), data])
        })?;
    } else if is(h, "removePrePlacement") && h.truthy(key) {
        let map = h.get(cache, "workerPrePlacementCache")?;
        method(h, &map, "delete", |_| Ok(vec![key.clone()]))?;
    } else if is(h, "seenFill") && h.truthy(key) {
        let set = h.get(cache, "workerSeenFillsSet")?;
        method(h, &set, "add", |_| Ok(vec![key.clone()]))?;
        let set = h.get(cache, "workerSeenFillsSet")?;
        let size = h.get(&set, "size")?;
        if h.greater_than_500(&size)? {
            let set = h.get(cache, "workerSeenFillsSet")?;
            let iterator = method(h, &set, "values", |_| Ok(vec![]))?;
            let set = h.get(cache, "workerSeenFillsSet")?;
            method(h, &set, "delete", |h| {
                let next = method(h, &iterator, "next", |_| Ok(vec![]))?;
                let first = h.get(&next, "value")?;
                Ok(vec![first])
            })?;
        }
    }
    Ok(())
}
/// ts: HyperliquidCrossCache.addPrePlacement.
pub fn add_pre_placement<H: CacheRuntimeHost>(
    h: &mut H,
    cache: &H::Value,
    coin: &H::Value,
    side: &H::Value,
    kind: &H::Value,
    price: &H::Value,
    amount: &H::Value,
) -> Result<(), H::Error> {
    let key = method(h, cache, "getPrePlacementKey", |_| {
        Ok(vec![coin.clone(), side.clone()])
    })?;
    let entry = h.object()?;
    h.define_data(&entry, &text("type"), kind)?;
    h.define_data(&entry, &text("price"), price)?;
    h.define_data(&entry, &text("amount"), amount)?;
    let timestamp = h.now()?;
    h.define_data(&entry, &text("timestamp"), &timestamp)?;
    let record = method(h, cache, "readPrePlacementCache", |_| Ok(vec![]))?;
    let current = h.get_index(&record, &key)?;
    if !h.truthy(&current) {
        let property = h.property_key(&key)?;
        let empty = h.array(&[])?;
        h.set(&record, &property, &empty)?;
    }
    let entries = h.get_index(&record, &key)?;
    method(h, &entries, "push", |_| Ok(vec![entry.clone()]))?;
    let now = h.now()?;
    let keys = h.object_keys(&record)?;
    let iterator = h.entries(&keys)?;
    while let Some(k) = h.iterator_step(&iterator)? {
        let body = (|| {
            let property = h.property_key(&k)?;
            let entries = h.get_index(&record, &k)?;
            let filtered = method(h, &entries, "filter", |h| {
                Ok(vec![h.function(now.clone(), Callback::AddValid)?])
            })?;
            h.set(&record, &property, &filtered)?;
            let entries = h.get_index(&record, &k)?;
            let length = h.get(&entries, "length")?;
            if h.equal(&length, &h.number(0.)) {
                h.delete(&record, &k)?;
            }
            Ok(())
        })();
        if let Err(error) = body {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    method(h, cache, "writePrePlacementCache", |_| {
        Ok(vec![record.clone()])
    })?;
    let map = h.get(cache, "workerPrePlacementCache")?;
    let entries = method(h, &map, "get", |_| Ok(vec![key.clone()]))?;
    let memory = if h.truthy(&entries) {
        entries
    } else {
        h.array(&[])?
    };
    method(h, &memory, "push", |_| Ok(vec![entry]))?;
    let map = h.get(cache, "workerPrePlacementCache")?;
    method(h, &map, "set", |_| Ok(vec![key.clone(), memory]))?;
    method(h, cache, "broadcastCacheUpdate", |h| {
        let action = h.literal(&text("addPrePlacement"));
        let data = h.get_index(&record, &key)?;
        Ok(vec![action, key, data])
    })?;
    Ok(())
}
/// ts: HyperliquidCrossCache.matchPrePlacement.
pub fn match_pre_placement<H: CacheRuntimeHost>(
    h: &mut H,
    cache: &H::Value,
    coin: &H::Value,
    side: &H::Value,
    price: &H::Value,
    amount: &H::Value,
) -> Result<H::Value, H::Error> {
    let key = method(h, cache, "getPrePlacementKey", |_| {
        Ok(vec![coin.clone(), side.clone()])
    })?;
    let record = method(h, cache, "readPrePlacementCache", |_| Ok(vec![]))?;
    let entries = h.get_index(&record, &key)?;
    if !h.truthy(&entries) {
        return Ok(h.undefined());
    }
    let length = h.get(&entries, "length")?;
    if h.equal(&length, &h.number(0.)) {
        return Ok(h.undefined());
    }
    let now = h.now()?;
    let valid = method(h, &entries, "filter", |h| {
        Ok(vec![h.function(now, Callback::MatchValid)?])
    })?;
    let length = h.get(&valid, "length")?;
    if h.equal(&length, &h.number(0.)) {
        h.delete(&record, &key)?;
        method(h, cache, "writePrePlacementCache", |_| Ok(vec![record]))?;
        let map = h.get(cache, "workerPrePlacementCache")?;
        method(h, &map, "delete", |_| Ok(vec![key.clone()]))?;
        method(h, cache, "broadcastCacheUpdate", |h| {
            Ok(vec![h.literal(&text("removePrePlacement")), key])
        })?;
        return Ok(h.undefined());
    }
    let minus = h.number(-1.);
    let mut index = minus.clone();
    if !h.is_undefined(price) && h.equal(&index, &minus) {
        index = method(h, &valid, "findIndex", |h| {
            Ok(vec![h.function(price.clone(), Callback::PriceMatch)?])
        })?;
    }
    if !h.is_undefined(amount) && h.equal(&index, &minus) {
        let matches = method(h, &valid, "filter", |h| {
            Ok(vec![h.function(amount.clone(), Callback::AmountMatch)?])
        })?;
        let length = h.get(&matches, "length")?;
        if h.equal(&length, &h.number(1.)) {
            index = method(h, &valid, "indexOf", |h| {
                let first = h.get(&matches, "0")?;
                Ok(vec![first])
            })?;
        }
    }
    if h.equal(&index, &minus) {
        index = h.number(0.);
    }
    let matched = h.get_index(&valid, &index)?;
    method(h, &valid, "splice", |h| Ok(vec![index, h.number(1.)]))?;
    let length = h.get(&valid, "length")?;
    if h.equal(&length, &h.number(0.)) {
        h.delete(&record, &key)?;
    } else {
        let property = h.property_key(&key)?;
        h.set(&record, &property, &valid)?;
    }
    method(h, cache, "writePrePlacementCache", |_| Ok(vec![record]))?;
    let length = h.get(&valid, "length")?;
    if h.equal(&length, &h.number(0.)) {
        let map = h.get(cache, "workerPrePlacementCache")?;
        method(h, &map, "delete", |_| Ok(vec![key.clone()]))?;
        method(h, cache, "broadcastCacheUpdate", |h| {
            Ok(vec![h.literal(&text("removePrePlacement")), key])
        })?;
    } else {
        let map = h.get(cache, "workerPrePlacementCache")?;
        method(h, &map, "set", |_| Ok(vec![key.clone(), valid.clone()]))?;
        method(h, cache, "broadcastCacheUpdate", |h| {
            Ok(vec![h.literal(&text("addPrePlacement")), key, valid])
        })?;
    }
    let result = h.object()?;
    let kind = h.get(&matched, "type")?;
    h.define_data(&result, &text("type"), &kind)?;
    let price = h.get(&matched, "price")?;
    h.define_data(&result, &text("price"), &price)?;
    Ok(result)
}
/// ts: getCrossCache; assignment occurs only after successful construction.
pub fn get_cross_cache<H: CacheRuntimeHost>(
    h: &mut H,
    module: &CacheModule<H::Value>,
) -> Result<H::Value, H::Error> {
    let current = module.singleton.borrow().clone();
    if let Some(value) = current {
        return Ok(value);
    }
    let cache = new_cache(h, module)?;
    *module.singleton.borrow_mut() = Some(cache.value.clone());
    Ok(cache.value)
}
/// ts: hasSeenFill, the module export wrapper.
pub fn has_seen_fill<H: CacheRuntimeHost>(
    h: &mut H,
    module: &CacheModule<H::Value>,
    fill: &H::Value,
) -> Result<H::Value, H::Error> {
    let cache = get_cross_cache(h, module)?;
    method(h, &cache, "hasSeenFill", |_| Ok(vec![fill.clone()]))
}
/// ts: markFillSeen, the module export wrapper.
pub fn mark_fill_seen<H: CacheRuntimeHost>(
    h: &mut H,
    module: &CacheModule<H::Value>,
    fill: &H::Value,
) -> Result<(), H::Error> {
    let cache = get_cross_cache(h, module)?;
    method(h, &cache, "markFillSeen", |_| Ok(vec![fill.clone()]))?;
    Ok(())
}
/// Function body dispatch composes the eighteen existing/new mapped methods.
pub fn run_callback<H: CacheRuntimeHost>(
    h: &mut H,
    body: Callback,
    capture: &H::Value,
    receiver: &H::Value,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let a = |index: usize| args.get(index).cloned().unwrap_or_else(|| h.undefined());
    let first = a(0);
    let second = a(1);
    let third = a(2);
    let fourth = a(3);
    let fifth = a(4);
    match body {
        Callback::Receive => {
            receive(h, capture, &first)?;
            Ok(h.undefined())
        }
        Callback::AddValid | Callback::MatchValid => {
            let timestamp = h.get(&first, "timestamp")?;
            let age = h.subtract(capture, &timestamp)?;
            let limit = h.number(30000.);
            let valid = if body == Callback::AddValid {
                h.less_equal(&age, &limit)?
            } else {
                h.less(&age, &limit)?
            };
            Ok(h.boolean(valid))
        }
        Callback::PriceMatch | Callback::AmountMatch => {
            let key = if body == Callback::PriceMatch {
                "price"
            } else {
                "amount"
            };
            let value = h.get(&first, key)?;
            Ok(h.boolean(h.equal(&value, capture)))
        }
        Callback::Method(body) => {
            match body {
                CacheMethod::Initialize => initialize_broadcast_channel(h, receiver),
                CacheMethod::ClearStorage => {
                    crate::cache_orders::clear_stale_session_storage(h, receiver)
                }
                CacheMethod::GetStorage => return Ok(cross_cache::get_session_storage(h)),
                CacheMethod::Broadcast => {
                    let channel = h.get(receiver, "orderCacheBroadcast")?;
                    if h.truthy(&channel) {
                        let _caught: Result<(), H::Error> = (|| {
                            let channel = h.get(receiver, "orderCacheBroadcast")?;
                            method(h, &channel, "postMessage", |h| {
                                let value = h.object()?;
                                h.define_data(&value, &text("action"), &first)?;
                                h.define_data(&value, &text("key"), &second)?;
                                h.define_data(&value, &text("data"), &third)?;
                                Ok(vec![value])
                            })?;
                            Ok(())
                        })();
                    }
                }
                CacheMethod::PreKey => {
                    let coin = h.abstract_string(&first)?;
                    let side = h.abstract_string(&second)?;
                    return Ok(h.literal(&cross_cache::get_pre_placement_key(&coin, &side)));
                }
                CacheMethod::ReadPre => return cross_cache::read_pre_placement_cache(h, receiver),
                CacheMethod::WritePre => {
                    cross_cache::write_pre_placement_cache(h, receiver, &first)?
                }
                CacheMethod::AddPre => {
                    add_pre_placement(h, receiver, &first, &second, &third, &fourth, &fifth)?
                }
                CacheMethod::MatchPre => {
                    return match_pre_placement(h, receiver, &first, &second, &third, &fourth);
                }
                CacheMethod::ClearPre => {
                    crate::cache_orders::clear_pre_placement(h, receiver, &first, &second)?
                }
                CacheMethod::ReadPending => {
                    return cross_cache::read_pending_orders_cache(h, receiver);
                }
                CacheMethod::WritePending => {
                    cross_cache::write_pending_orders_cache(h, receiver, &first)?
                }
                CacheMethod::GetPending => {
                    return crate::cache_orders::get_pending_order(h, receiver, &first);
                }
                CacheMethod::AddPending => {
                    crate::cache_orders::add_pending_order(h, receiver, &first, &second, &third)?
                }
                CacheMethod::RemovePending => {
                    crate::cache_orders::remove_pending_order(h, receiver, &first)?
                }
                CacheMethod::HasFill => {
                    let set = h.get(receiver, "workerSeenFillsSet")?;
                    return method(h, &set, "has", |_| Ok(vec![first]));
                }
                CacheMethod::MarkFill => {
                    let set = h.get(receiver, "workerSeenFillsSet")?;
                    method(h, &set, "add", |_| Ok(vec![first.clone()]))?;
                    let set = h.get(receiver, "workerSeenFillsSet")?;
                    let size = h.get(&set, "size")?;
                    if h.greater_than_500(&size)? {
                        let set = h.get(receiver, "workerSeenFillsSet")?;
                        let iterator = method(h, &set, "values", |_| Ok(vec![]))?;
                        let set = h.get(receiver, "workerSeenFillsSet")?;
                        method(h, &set, "delete", |h| {
                            let next = method(h, &iterator, "next", |_| Ok(vec![]))?;
                            let key = h.get(&next, "value")?;
                            Ok(vec![key])
                        })?;
                    }
                    method(h, receiver, "broadcastCacheUpdate", |h| {
                        Ok(vec![h.literal(&text("seenFill")), first])
                    })?;
                }
                CacheMethod::Dispose => cross_cache::dispose(h, receiver)?,
            }
            Ok(h.undefined())
        }
    }
}
