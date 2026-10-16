//! Private-feed normalization, explicit closes and live throttled reconciliation.
use crate::{
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
type Task<V> = Box<dyn FnOnce() -> SigningResult<NativePromise<V>>>;

pub struct LighterPrivateFeedRuntime<H, V> {
    pub promise_runtime: Rc<dyn TransactionRuntime>,
    pub host: H,
    pub get: fn(&H, &V, &[u16]) -> SigningResult<V>,
    pub define: fn(&H, &V, &[u16], V) -> SigningResult<()>,
    pub method: fn(&H, V, &'static str) -> SigningResult<V>,
    pub call: fn(&H, V, Vec<V>) -> SigningResult<V>,
    pub call_async: fn(&H, V, Vec<V>) -> SigningResult<NativePromise<V>>,
    pub clear_timeout: fn(&H, V) -> SigningResult<V>,
    pub set_timeout: fn(&H, Box<dyn FnOnce() -> NativePromise<V>>, V) -> SigningResult<V>,
    pub schedule_call: fn(
        &Rc<LighterPrivateFeedRuntime<H, V>>,
        V,
        &'static str,
        V,
        Task<V>,
        V,
    ) -> SigningResult<V>,
    pub console_error: fn(&H, V, &TransactionSigningError) -> SigningResult<V>,
    pub stringify: fn(&H, V) -> SigningResult<V>,
    pub to_string: fn(&H, V) -> SigningResult<Vec<u16>>,
    pub trim_upper: fn(&H, Vec<u16>) -> SigningResult<Vec<u16>>,
    pub to_number: fn(&H, V) -> SigningResult<f64>,
    pub parse_float: fn(&H, Vec<u16>) -> SigningResult<f64>,
    pub copy: fn(&H, V) -> SigningResult<V>,
    pub map: fn(&H, V, &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V>,
    pub filter: fn(&H, V, &mut dyn FnMut(V) -> SigningResult<bool>) -> SigningResult<V>,
    pub now: fn(&H) -> SigningResult<f64>,
    pub number: fn(f64) -> V,
    pub text: fn(&str) -> V,
    pub utf16: fn(Vec<u16>) -> V,
    pub boolean: fn(bool) -> V,
    pub undefined: fn() -> V,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub strict_same: fn(&V, &V) -> bool,
}
fn get<H, V>(r: &LighterPrivateFeedRuntime<H, V>, value: &V, key: &str) -> SigningResult<V> {
    (r.get)(&r.host, value, &key.encode_utf16().collect::<Vec<_>>())
}
fn define<H, V>(
    r: &LighterPrivateFeedRuntime<H, V>,
    value: &V,
    key: &str,
    field: V,
) -> SigningResult<()> {
    (r.define)(
        &r.host,
        value,
        &key.encode_utf16().collect::<Vec<_>>(),
        field,
    )
}
fn coalesce<H, V>(
    r: &LighterPrivateFeedRuntime<H, V>,
    value: &V,
    keys: &[&str],
    fallback: Option<V>,
) -> SigningResult<V> {
    let mut selected = (r.undefined)();
    for key in keys {
        selected = get(r, value, key)?;
        if !(r.nullish)(&selected) {
            return Ok(selected);
        }
    }
    Ok(fallback.unwrap_or(selected))
}
fn normalized<H, V>(r: &LighterPrivateFeedRuntime<H, V>, value: V) -> SigningResult<Vec<u16>> {
    (r.trim_upper)(&r.host, (r.to_string)(&r.host, value)?)
}

pub fn normalize_order_status<H, V>(
    r: &LighterPrivateFeedRuntime<H, V>,
    order: &V,
) -> SigningResult<V> {
    let value = coalesce(
        r,
        order,
        &["status", "order_status", "state"],
        Some((r.text)("")),
    )?;
    Ok((r.utf16)(normalized(r, value)?))
}
pub fn is_user_cancelled_reason<H, V>(
    r: &LighterPrivateFeedRuntime<H, V>,
    reason: V,
) -> SigningResult<bool> {
    let value = if (r.nullish)(&reason) {
        (r.text)("")
    } else {
        reason
    };
    let normalized = normalized(r, value)?;
    Ok(
        normalized == "USER_CANCELLED".encode_utf16().collect::<Vec<_>>()
            || normalized == "USER_CANCELED".encode_utf16().collect::<Vec<_>>(),
    )
}

pub fn resolve_explicitly_closed_position_updates<H, V: Clone>(
    r: &LighterPrivateFeedRuntime<H, V>,
    owner: &V,
    raw: &V,
) -> SigningResult<V> {
    let size = coalesce(
        r,
        raw,
        &["size", "position", "qty", "contracts"],
        Some((r.text)("")),
    )?;
    let size = (r.parse_float)(&r.host, (r.to_string)(&r.host, size)?)?;
    if !size.is_finite() || size.abs() > 0.0 {
        return Ok((r.array)(vec![]));
    }
    let market_id = coalesce(
        r,
        raw,
        &["marketId", "market_id", "marketIndex", "market_index"],
        None,
    )?;
    let market = if !(r.is_undefined)(&market_id) {
        let store = get(r, owner, "store")?;
        let markets = get(r, &store, "marketsById")?;
        let key = (r.to_string)(&r.host, market_id)?;
        (r.get)(&r.host, &markets, &key)?
    } else {
        (r.undefined)()
    };
    let symbol = get(r, raw, "symbol")?;
    let symbol = if !(r.nullish)(&symbol) {
        symbol
    } else {
        let from_market = if (r.nullish)(&market) {
            (r.undefined)()
        } else {
            get(r, &market, "symbol")?
        };
        if (r.nullish)(&from_market) {
            (r.text)("")
        } else {
            from_market
        }
    };
    let symbol = (r.to_string)(&r.host, symbol)?;
    if symbol.is_empty() {
        return Ok((r.array)(vec![]));
    }
    let symbol = (r.utf16)(symbol);
    let side = coalesce(
        r,
        raw,
        &["side", "positionSide", "position_side"],
        Some((r.text)("")),
    )?;
    let side = normalized(r, side)?;
    let sign = coalesce(
        r,
        raw,
        &["sign", "positionSign", "position_sign"],
        Some((r.number)(f64::NAN)),
    )?;
    let sign = (r.to_number)(&r.host, sign)?;
    let equals = |word: &str| side == word.encode_utf16().collect::<Vec<_>>();
    // The repeated `short`/`long` bodies are four DISTINCT source branches
    // (explicit side words, then sign fallbacks). Collapsing them would merge
    // conditions the TypeScript keeps separate and lose the 1:1 mapping the
    // parity manifest rows point at.
    #[allow(clippy::if_same_then_else)]
    let side_hint = if equals("LONG") || equals("BUY") {
        (r.text)("long")
    } else if equals("SHORT") || equals("SELL") {
        (r.text)("short")
    } else if sign.is_finite() && sign < 0.0 {
        (r.text)("short")
    } else if sign.is_finite() && sign > 0.0 {
        (r.text)("long")
    } else {
        (r.undefined)()
    };
    let store = get(r, owner, "store")?;
    let positions = get(r, &store, "positions")?;
    let filter = (r.method)(&r.host, positions, "filter")?;
    let targets = (r.filter)(&r.host, filter, &mut |position| {
        Ok((r.strict_same)(&get(r, &position, "symbol")?, &symbol)
            && !(r.strict_same)(&get(r, &position, "contracts")?, &(r.number)(0.0))
            && (!(r.truthy)(&side_hint)
                || (r.strict_same)(&get(r, &position, "side")?, &side_hint)))
    })?;
    if !(r.truthy)(&get(r, &targets, "length")?) {
        return Ok((r.array)(vec![]));
    }
    let realized = coalesce(
        r,
        raw,
        &["realizedPnl", "realized_pnl", "realized_profit"],
        Some((r.text)("")),
    )?;
    let realized = (r.parse_float)(&r.host, (r.to_string)(&r.host, realized)?)?;
    let now = (r.now)(&r.host)?;
    let map = (r.method)(&r.host, targets, "map")?;
    (r.map)(&r.host, map, &mut |position| {
        let updated = (r.copy)(&r.host, position.clone())?;
        for key in [
            "entryPrice",
            "notional",
            "unrealizedPnl",
            "contracts",
            "liquidationPrice",
        ] {
            define(r, &updated, key, (r.number)(0.0))?;
        }
        let pnl = if realized.is_finite() {
            (r.number)(realized)
        } else {
            get(r, &position, "realizedPnl")?
        };
        define(r, &updated, "realizedPnl", pnl)?;
        define(r, &updated, "info", raw.clone())?;
        define(r, &updated, "lastUpdate", (r.number)(now))?;
        Ok((r.array)(vec![position, updated]))
    })
}

fn finish_timer<H, V>(
    r: &LighterPrivateFeedRuntime<H, V>,
    prefix: V,
    error: Option<TransactionSigningError>,
    out: PromiseResolver<V>,
) where
    V: 'static,
{
    let result = if let Some(error) = error {
        (r.console_error)(&r.host, prefix, &error).map(|_| (r.undefined)())
    } else {
        Ok((r.undefined)())
    };
    out.settle(result);
}
pub fn schedule_throttled_task<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: &V,
    key: &'static str,
    delay: V,
    task: Task<V>,
    prefix: V,
) -> SigningResult<V> {
    let existing = get(r, owner, key)?;
    if (r.truthy)(&existing) {
        (r.clear_timeout)(&r.host, existing)?;
    }
    let callback_r = r.clone();
    let callback_owner = owner.clone();
    let callback = Box::new(move || {
        let r = callback_r;
        let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
        let active = (|| {
            define(&r, &callback_owner, key, (r.undefined)())?;
            Ok(!(r.truthy)(&get(&r, &callback_owner, "isDisposed")?))
        })();
        match active {
            Err(error) => out.settle(Err(error)),
            Ok(false) => out.settle(Ok((r.undefined)())),
            Ok(true) => match task() {
                Err(error) => finish_timer(&r, prefix, Some(error), out),
                Ok(task) => task.subscribe(move |value| finish_timer(&r, prefix, value.err(), out)),
            },
        }
        promise
    });
    let timer = (r.set_timeout)(&r.host, callback, delay)?;
    define(r, owner, key, timer)?;
    Ok((r.undefined)())
}

fn fetch_body<H: 'static, V: Clone + 'static>(
    r: Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: V,
    kind: &'static str,
    opts: V,
) -> NativePromise<V> {
    let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
    let started = (|| {
        if kind == "fetchOrders" && (r.truthy)(&get(&r, &opts, "skipIfRecent")?) {
            let parent = get(&r, &owner, "parent")?;
            let skip = (r.method)(&r.host, parent, "shouldSkipOrderReconciliationFetch")?;
            if (r.truthy)(&(r.call)(&r.host, skip, vec![])?) {
                return Ok(None);
            }
        }
        let parent = get(&r, &owner, "parent")?;
        let fetch = (r.method)(&r.host, parent, kind)?;
        let args = if kind == "fetchAndSetBalance" {
            vec![(r.object)(vec![
                ("allowRecentAccountSnapshot", (r.boolean)(false)),
                ("includeAccountLimits", (r.boolean)(false)),
            ])]
        } else {
            vec![]
        };
        (r.call_async)(&r.host, fetch, args).map(Some)
    })();
    match started {
        Err(error) => out.settle(Err(error)),
        Ok(None) => out.settle(Ok((r.undefined)())),
        Ok(Some(fetch)) => fetch.subscribe(move |value| match value {
            Err(error) => out.settle(Err(error)),
            Ok(value) => {
                let completed = (|| {
                    if kind == "fetchAndSetBalance" {
                        return Ok((r.undefined)());
                    }
                    if kind == "fetchOrders" {
                        if (r.truthy)(&get(&r, &owner, "isDisposed")?) {
                            return Ok((r.undefined)());
                        }
                        let parent = get(&r, &owner, "parent")?;
                        if (r.truthy)(&get(&r, &parent, "isDisposed")?) {
                            return Ok((r.undefined)());
                        }
                    }
                    let parent = get(&r, &owner, "parent")?;
                    let store = get(&r, &parent, "store")?;
                    let update = (r.method)(
                        &r.host,
                        store,
                        if kind == "fetchOrders" {
                            "addOrUpdateOrdersFull"
                        } else {
                            "addOrUpdatePositionsFull"
                        },
                    )?;
                    (r.call)(&r.host, update, vec![(*value).clone()])?;
                    if kind == "fetchPositions" {
                        let parent = get(&r, &owner, "parent")?;
                        let now = (r.now)(&r.host)?;
                        define(&r, &parent, "lastWebsocketPositionsUpdate", (r.number)(now))?;
                    }
                    Ok((r.undefined)())
                })();
                out.settle(completed);
            }
        }),
    }
    promise
}
fn throttle<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: &V,
    delay: V,
    kind: &'static str,
    key: &'static str,
    opts: V,
    prefix: &'static str,
) -> SigningResult<V> {
    let delay = if (r.is_undefined)(&delay) {
        (r.number)(500.0)
    } else {
        delay
    };
    let scheduler = (r.method)(&r.host, owner.clone(), "scheduleThrottledTask")?;
    let callback_r = r.clone();
    let callback_owner = owner.clone();
    (r.schedule_call)(
        r,
        scheduler,
        key,
        delay,
        Box::new(move || Ok(fetch_body(callback_r, callback_owner, kind, opts))),
        (r.text)(prefix),
    )?;
    Ok((r.undefined)())
}
pub fn throttle_fetch_orders<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: &V,
    delay: V,
    opts: V,
) -> SigningResult<V> {
    let opts = if (r.is_undefined)(&opts) {
        (r.object)(vec![])
    } else {
        opts
    };
    if (r.truthy)(&get(r, &opts, "skipIfRecent")?) {
        let parent = get(r, owner, "parent")?;
        let skip = (r.method)(&r.host, parent, "shouldSkipOrderReconciliationFetch")?;
        if (r.truthy)(&(r.call)(&r.host, skip, vec![])?) {
            return Ok((r.undefined)());
        }
    }
    throttle(
        r,
        owner,
        delay,
        "fetchOrders",
        "ordersThrottleTimeout",
        opts,
        "[Lighter] Error fetching orders:",
    )
}
pub fn throttle_fetch_positions<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: &V,
    delay: V,
) -> SigningResult<V> {
    throttle(
        r,
        owner,
        delay,
        "fetchPositions",
        "positionsThrottleTimeout",
        (r.undefined)(),
        "[Lighter] Error fetching positions:",
    )
}
pub fn throttle_fetch_balance<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateFeedRuntime<H, V>>,
    owner: &V,
    delay: V,
) -> SigningResult<V> {
    throttle(
        r,
        owner,
        delay,
        "fetchAndSetBalance",
        "balanceThrottleTimeout",
        (r.undefined)(),
        "[Lighter] Error fetching balance:",
    )
}
pub fn send_subscribe_payload<H, V: Clone>(
    r: &LighterPrivateFeedRuntime<H, V>,
    owner: &V,
    payload: V,
    auth: bool,
) -> SigningResult<V> {
    let parent = get(r, owner, "parent")?;
    let log = (r.method)(&r.host, parent, "log")?;
    let channel = get(r, &payload, "channel")?;
    let channel = if (r.truthy)(&channel) {
        channel
    } else {
        (r.text)("")
    };
    let mut message: Vec<u16> = "[Lighter][PWS] Sending subscribe for channel "
        .encode_utf16()
        .collect();
    message.extend((r.to_string)(&r.host, channel)?);
    message.extend(format!(" (auth={}).", if auth { "yes" } else { "no" }).encode_utf16());
    (r.call)(&r.host, log, vec![(r.utf16)(message), (r.text)("info")])?;
    let send = (r.method)(&r.host, owner.clone(), "sendWithLog")?;
    let serialized = (r.stringify)(&r.host, payload)?;
    (r.call)(&r.host, send, vec![serialized])?;
    Ok((r.undefined)())
}
