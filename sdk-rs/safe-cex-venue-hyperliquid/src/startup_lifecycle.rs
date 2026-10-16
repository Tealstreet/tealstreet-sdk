//! Primary exchange startup and periodic refresh bodies with explicit native bindings.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::{AccountCallback, SpotAccountHost},
    spot_json::JsString,
};
#[derive(Clone)]
pub enum StartupCallback<V> {
    Warning {
        exchange: V,
        prefix: &'static str,
        returns_null: bool,
    },
    CoreState {
        exchange: V,
    },
}
pub trait StartupLifecycleHost: SpotAccountHost {
    /// Allocate a lexical native closure; its body is `run_startup_callback`.
    fn startup_callback(
        &mut self,
        callback: StartupCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    object: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let callee = h.get(object, key)?;
    let args = args(h)?;
    h.call(&callee, object, &args)
}
fn log<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    message: &str,
    severity: &str,
) -> Result<H::Value, H::Error> {
    method(h, e, "log", |h| {
        let message = lit(h, message);
        let severities = h.imported("LogSeverity")?;
        let severity = h.get(&severities, severity)?;
        Ok(vec![message, severity])
    })
}
fn now<H: SpotAccountHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let date = h.global("Date")?;
    method(h, &date, "now", |_| Ok(vec![]))
}
fn caught<H: StartupLifecycleHost>(
    h: &mut H,
    e: &H::Value,
    name: &str,
    prefix: &'static str,
    returns_null: bool,
) -> Result<H::Value, H::Error> {
    let promise = method(h, e, name, |_| Ok(vec![]))?;
    method(h, &promise, "catch", |h| {
        Ok(vec![h.startup_callback(StartupCallback::Warning {
            exchange: e.clone(),
            prefix,
            returns_null,
        })?])
    })
}
pub fn run_startup_callback<H: StartupLifecycleHost>(
    h: &mut H,
    callback: &StartupCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match callback {
        StartupCallback::Warning {
            exchange,
            prefix,
            returns_null,
        } => {
            method(h, exchange, "log", |h| {
                let mut text = prefix.encode_utf16().collect::<Vec<_>>();
                text.extend(h.template_string(&first)?.0);
                let message = h.literal(&JsString(text));
                let severities = h.imported("LogSeverity")?;
                let severity = h.get(&severities, "Warning")?;
                Ok(vec![message, severity])
            })?;
            Ok(if *returns_null {
                h.null()
            } else {
                h.undefined()
            })
        }
        StartupCallback::CoreState { exchange } => {
            let disposed = h.get(exchange, "isDisposed")?;
            if h.truthy(&disposed) || !h.truthy(&first) {
                return Ok(h.undefined());
            }
            let socket = h.get(exchange, "privateWebsocket")?;
            method(h, &socket, "applyCoreClearinghouseState", |h| {
                Ok(vec![first, h.boolean(true)])
            })?;
            Ok(h.undefined())
        }
    }
}
async fn start_body<H: StartupLifecycleHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error> {
    let init = method(&mut h, &e, "initExternalPublicData", |_| Ok(vec![]))?;
    let external = h.await_value(init).await?;
    let options = h.get(&e, "options")?;
    let only = h.get(&options, "publicDataOnly")?;
    let explicit = h.strict_equal(&only, &h.boolean(true));
    if !explicit {
        let identity = h.get(&e, "hasPrivateSessionIdentity")?;
        if !h.truthy(&identity) {
            let options = h.get(&e, "options")?;
            let key = h.get(&options, "key")?;
            if h.truthy(&key) {
                let resolution = method(&mut h, &e, "resolveApplicationIdFromKey", |_| Ok(vec![]))?;
                h.await_value(resolution).await?;
                let disposed = h.get(&e, "isDisposed")?;
                if h.truthy(&disposed) {
                    return Ok(h.undefined());
                }
            }
        }
    }
    method(&mut h, &e, "warnOnMismatchedApiWallet", |h| {
        Ok(vec![h.boolean(explicit)])
    })?;
    let private = if explicit {
        false
    } else {
        let identity = h.get(&e, "hasPrivateSessionIdentity")?;
        h.truthy(&identity)
    };
    let mark_loaded = explicit || private;
    if !private && !explicit {
        log(
            &mut h,
            &e,
            "Skipping private startup: no applicationId/vault configured",
            "Warning",
        )?;
    }
    let _initial_markets = h.array(&[])?;
    let _initial_tickers = h.array(&[])?;
    let mode = if private {
        caught(
            &mut h,
            &e,
            "resolveUserAbstractionMode",
            "userAbstraction resolution failed: ",
            false,
        )?
    } else {
        h.null()
    };
    let core = if private {
        caught(
            &mut h,
            &e,
            "fetchCorePerpsState",
            "Initial core-perp account-state fetch failed: ",
            true,
        )?
    } else {
        h.null()
    };
    if private {
        caught(
            &mut h,
            &e,
            "fetchBuilderFeeApproval",
            "Builder fee approval check failed: ",
            false,
        )?;
    }
    let (markets, tickers) = if !h.truthy(&external) {
        let fetched = method(&mut h, &e, "fetchMarketsAndTickers", |_| Ok(vec![]))?;
        let fetched = h.await_value(fetched).await?;
        let disposed = h.get(&e, "isDisposed")?;
        if h.truthy(&disposed) {
            return Ok(h.undefined());
        }
        let markets = h.get(&fetched, "0")?;
        let tickers = h.get(&fetched, "1")?;
        let socket = h.get(&e, "publicWebsocket")?;
        method(&mut h, &socket, "connectAndSubscribe", |_| Ok(vec![]))?;
        (markets, tickers)
    } else {
        let store = h.get(&e, "store")?;
        let markets = h.get(&store, "markets")?;
        let store = h.get(&e, "store")?;
        let tickers = h.get(&store, "tickers")?;
        (markets, tickers)
    };
    method(&mut h, &e, "buildMarketsLowerCaseMap", |_| {
        Ok(vec![markets.clone()])
    })?;
    let positions = method(&mut h, &e, "generateFakePositionsFromMarkets", |_| {
        Ok(vec![markets.clone()])
    })?;
    let update = h.object()?;
    h.define_data(&update, "positions", &positions)?;
    let orders = h.array(&[])?;
    h.define_data(&update, "orders", &orders)?;
    let balance = h.object()?;
    for key in ["used", "free", "total", "upnl"] {
        h.define_data(&balance, key, &h.number(0.))?;
    }
    h.define_data(&update, "balance", &balance)?;
    let loaded = h.object()?;
    let store = h.get(&e, "store")?;
    let old_loaded = h.get(&store, "loaded")?;
    h.spread_into(&loaded, &old_loaded)?;
    for key in ["orders", "balance", "positions"] {
        h.define_data(&loaded, key, &h.boolean(mark_loaded))?;
    }
    h.define_data(&update, "loaded", &loaded)?;
    if !h.truthy(&external) {
        h.set(&update, "markets", &markets)?;
        h.set(&update, "tickers", &tickers)?;
        let loaded = h.get(&update, "loaded")?;
        h.set(&loaded, "markets", &h.boolean(true))?;
        let loaded = h.get(&update, "loaded")?;
        h.set(&loaded, "tickers", &h.boolean(true))?;
    }
    let store = h.get(&e, "store")?;
    method(&mut h, &store, "update", |_| Ok(vec![update]))?;
    if private {
        h.await_value(mode).await?;
        let disposed = h.get(&e, "isDisposed")?;
        if h.truthy(&disposed) {
            return Ok(h.undefined());
        }
        let socket = h.get(&e, "privateWebsocket")?;
        method(&mut h, &socket, "connectAndSubscribe", |_| Ok(vec![]))?;
        let track = method(&mut h, &e, "shouldTrackSpotHoldings", |_| Ok(vec![]))?;
        if h.truthy(&track) {
            method(&mut h, &e, "requestUnifiedSpotCollateralRefresh", |h| {
                Ok(vec![h.boolean(true)])
            })?;
        }
    }
    if h.truthy(&core) {
        method(&mut h, &core, "then", |h| {
            Ok(vec![h.startup_callback(StartupCallback::CoreState {
                exchange: e.clone(),
            })?])
        })?;
    }
    method(&mut h, &e, "log", |h| {
        Ok(vec![lit(h, "Ready to watch public data on Hyperliquid")])
    })?;
    let only = h.get(&e, "isPublicDataOnly")?;
    if h.truthy(&only) {
        return Ok(h.undefined());
    }
    let time = now(&mut h)?;
    h.set(&e, "_lastOrderRefreshTime", &time)?;
    caught(
        &mut h,
        &e,
        "refreshOrders",
        "Failed to fetch initial orders: ",
        false,
    )?;
    let time = now(&mut h)?;
    h.set(&e, "_lastPositionRefreshTime", &time)?;
    caught(
        &mut h,
        &e,
        "fetchAllHip3Positions",
        "Failed to fetch initial HIP-3 positions: ",
        false,
    )?;
    let tick = method(&mut h, &e, "tick", |_| Ok(vec![]))?;
    h.await_value(tick).await?;
    let disposed = h.get(&e, "isDisposed")?;
    if h.truthy(&disposed) {
        return Ok(h.undefined());
    }
    Ok(h.undefined())
}
async fn tick_body<H: StartupLifecycleHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error> {
    let disposed = h.get(&e, "isDisposed")?;
    if !h.truthy(&disposed) {
        let only = h.get(&e, "isPublicDataOnly")?;
        if h.truthy(&only) {
            return Ok(h.undefined());
        }
        let time = now(&mut h)?;
        let refresh = h.get(&e, "_hip3OrdersNeedRefresh")?;
        if h.truthy(&refresh) {
            let dexes = h.get(&e, "hip3DexNames")?;
            let length = h.get(&dexes, "length")?;
            let has_dex = h.binary(">", &length, &h.number(0.))?;
            if h.truthy(&has_dex) {
                let last = h.get(&e, "_lastOrderRefreshTime")?;
                let elapsed = h.binary("-", &time, &last)?;
                let interval = h.get(&e, "HIP3_ORDER_REFRESH_INTERVAL_MS")?;
                let due = h.binary(">", &elapsed, &interval)?;
                if h.truthy(&due) {
                    h.set(&e, "_hip3OrdersNeedRefresh", &h.boolean(false))?;
                    h.set(&e, "_lastOrderRefreshTime", &time)?;
                    log(
                        &mut h,
                        &e,
                        "Refreshing HIP-3 orders (new order detected via WS)",
                        "Debug",
                    )?;
                    caught(
                        &mut h,
                        &e,
                        "refreshOrders",
                        "HIP-3 order refresh failed: ",
                        false,
                    )?;
                }
            }
        }
        let dexes = h.get(&e, "hip3DexNames")?;
        let length = h.get(&dexes, "length")?;
        let has_dex = h.binary(">", &length, &h.number(0.))?;
        if h.truthy(&has_dex) {
            let last = h.get(&e, "_lastPositionRefreshTime")?;
            let elapsed = h.binary("-", &time, &last)?;
            let interval = h.get(&e, "HIP3_POSITION_REFRESH_INTERVAL_MS")?;
            let due = h.binary(">", &elapsed, &interval)?;
            if h.truthy(&due) {
                h.set(&e, "_lastPositionRefreshTime", &time)?;
                log(&mut h, &e, "Periodic HIP-3 position refresh", "Debug")?;
                caught(
                    &mut h,
                    &e,
                    "fetchAllHip3Positions",
                    "Periodic HIP-3 position refresh failed: ",
                    false,
                )?;
            }
        }
        let disposed = h.get(&e, "isDisposed")?;
        if !h.truthy(&disposed) {
            let loop_fn = h.imported("loop")?;
            let callback = SpotAccountHost::callback(
                &mut h,
                AccountCallback::Tick {
                    exchange: e.clone(),
                },
            )?;
            let interval = method(&mut h, &e, "getTickInterval", |_| Ok(vec![]))?;
            h.call(&loop_fn, &h.undefined(), &[callback, interval])?;
        }
    }
    Ok(h.undefined())
}
pub fn start<H>(host: H, exchange: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: StartupLifecycleHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, start_body(host, exchange))
}
pub fn tick<H>(host: H, exchange: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: StartupLifecycleHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, tick_body(host, exchange))
}
