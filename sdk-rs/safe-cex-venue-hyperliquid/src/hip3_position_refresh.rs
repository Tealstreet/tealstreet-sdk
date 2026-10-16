//! HIP-3 position refresh triggers and their owned rejection/debounce callbacks.
use crate::spot_account_reads::SpotAccountHost;
#[derive(Clone)]
pub enum PositionRefreshCallback<V> {
    MapDex { exchange: V },
    DexError { exchange: V, dex: V },
    IgnoreAggregate,
    Debounce { exchange: V },
}
/// Runtime bindings retain the lexical receiver and shared runtime in owned callbacks.
/// Iterator records cache their native next method/owner; closing after a body
/// throw implements IteratorClose with the original throw taking precedence.
/// These operations are bindings, not a replacement implementation of JS intrinsics.
pub trait PositionRefreshHost: SpotAccountHost {
    fn position_refresh_callback(
        &mut self,
        capture: PositionRefreshCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, iterator: &Self::Value);
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    o: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, key)?;
    let args = args(h)?;
    h.call(&f, o, &args)
}
fn empty<H: SpotAccountHost>(h: &mut H, set: &H::Value) -> Result<bool, H::Error> {
    let size = h.get(set, "size")?;
    Ok(h.strict_equal(&size, &h.number(0.)))
}
pub fn run_position_refresh_callback<H: PositionRefreshHost>(
    h: &mut H,
    capture: &PositionRefreshCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match capture {
        PositionRefreshCallback::MapDex { exchange } => {
            let promise = method(h, exchange, "fetchHip3Positions", |_| {
                Ok(vec![first.clone()])
            })?;
            method(h, &promise, "catch", |h| {
                Ok(vec![h.position_refresh_callback(
                    PositionRefreshCallback::DexError {
                        exchange: exchange.clone(),
                        dex: first,
                    },
                )?])
            })
        }
        PositionRefreshCallback::DexError { exchange, dex } => {
            method(h, exchange, "log", |h| {
                let mut text = "Failed to refresh HIP-3 positions for "
                    .encode_utf16()
                    .collect::<Vec<_>>();
                text.extend(h.template_string(dex)?.0);
                text.extend(": ".encode_utf16());
                text.extend(h.template_string(&first)?.0);
                let message = h.literal(&crate::spot_json::JsString(text));
                let table = h.imported("LogSeverity")?;
                let severity = h.get(&table, "Warning")?;
                Ok(vec![message, severity])
            })?;
            Ok(h.undefined())
        }
        PositionRefreshCallback::IgnoreAggregate => Ok(h.undefined()),
        PositionRefreshCallback::Debounce { exchange } => {
            h.set(exchange, "_hip3PositionRefreshDebounceTimer", &h.null())?;
            let disposed = h.get(exchange, "isDisposed")?;
            if h.truthy(&disposed) {
                return Ok(h.undefined());
            }
            let pending = h.get(exchange, "_pendingHip3PositionRefreshDexes")?;
            if empty(h, &pending)? {
                return Ok(h.undefined());
            }
            let constructor = h.global("Set")?;
            let pending = h.get(exchange, "_pendingHip3PositionRefreshDexes")?;
            let snapshot = h.construct(&constructor, &[pending])?;
            let pending = h.get(exchange, "_pendingHip3PositionRefreshDexes")?;
            method(h, &pending, "clear", |_| Ok(vec![]))?;
            method(h, exchange, "refreshHip3PositionsAsync", |_| {
                Ok(vec![snapshot])
            })?;
            Ok(h.undefined())
        }
    }
}
pub fn refresh_hip3_positions_async<H: PositionRefreshHost>(
    h: &mut H,
    e: &H::Value,
    dex_names: &H::Value,
) -> Result<H::Value, H::Error> {
    if empty(h, dex_names)? {
        return Ok(h.undefined());
    }
    let date = h.global("Date")?;
    let now = method(h, &date, "now", |_| Ok(vec![]))?;
    h.set(e, "_lastPositionRefreshTime", &now)?;
    let promise = h.global("Promise")?;
    let aggregate = method(h, &promise, "all", |h| {
        let array = h.global("Array")?;
        let values = method(h, &array, "from", |_| Ok(vec![dex_names.clone()]))?;
        let mapped = method(h, &values, "map", |h| {
            Ok(vec![h.position_refresh_callback(
                PositionRefreshCallback::MapDex {
                    exchange: e.clone(),
                },
            )?])
        })?;
        Ok(vec![mapped])
    })?;
    method(h, &aggregate, "catch", |h| {
        Ok(vec![h.position_refresh_callback(
            PositionRefreshCallback::IgnoreAggregate,
        )?])
    })?;
    Ok(h.undefined())
}
pub fn schedule_hip3_position_refresh<H: PositionRefreshHost>(
    h: &mut H,
    e: &H::Value,
    dex_names: &H::Value,
) -> Result<H::Value, H::Error> {
    if empty(h, dex_names)? {
        return Ok(h.undefined());
    }
    let disposed = h.get(e, "isDisposed")?;
    if h.truthy(&disposed) {
        return Ok(h.undefined());
    }
    let iterator = h.iterator(dex_names)?;
    while let Some(dex) = h.next(&iterator)? {
        let step = (|| {
            if h.truthy(&dex) {
                let pending = h.get(e, "_pendingHip3PositionRefreshDexes")?;
                method(h, &pending, "add", |_| Ok(vec![dex]))?;
            }
            Ok(())
        })();
        if let Err(error) = step {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    let timer = h.get(e, "_hip3PositionRefreshDebounceTimer")?;
    if h.truthy(&timer) {
        let clear = h.global("clearTimeout")?;
        let timer = h.get(e, "_hip3PositionRefreshDebounceTimer")?;
        h.call(&clear, &h.undefined(), &[timer])?;
    }
    let set = h.global("setTimeout")?;
    let callback = h.position_refresh_callback(PositionRefreshCallback::Debounce {
        exchange: e.clone(),
    })?;
    let interval = h.get(e, "HIP3_POSITION_REFRESH_DEBOUNCE_MS")?;
    let timer = h.call(&set, &h.undefined(), &[callback, interval])?;
    h.set(e, "_hip3PositionRefreshDebounceTimer", &timer)?;
    Ok(h.undefined())
}
