//! Exchange constructor and account lifecycle bodies over receiver-owned capabilities.
use crate::types::ENDPOINTS;
use sha2::{Digest, Sha256};
use std::{future::Future, pin::Pin};
type F<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
#[derive(Clone, Copy)]
pub enum WeexLifecycleCallback {
    ValidationBalance,
    StartBalancePositions,
    StartOrders,
    Tick,
}
/// Factories start host-owned work; all/then retain peers after observer rejection/drop.
pub struct WeexLifecycleRuntime<H, V, E> {
    pub base_construct: fn(&H, V, V, V) -> Result<V, E>,
    pub client_rule: fn(&H, &V) -> Result<V, E>,
    pub api_factory: fn(&H) -> Result<V, E>,
    pub bind: fn(&H, &V, &'static str) -> Result<V, E>,
    pub new_map: fn(&H) -> Result<V, E>,
    pub assign: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub constant: fn(&H, &'static str) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub this: fn(&H) -> V,
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call: fn(&H, V, Vec<V>) -> Result<V, E>,
    pub call_async: for<'a> fn(&'a H, V, Vec<V>) -> Result<F<'a, V, E>, E>,
    pub construct: fn(&H, &'static str, V) -> Result<V, E>,
    pub then: for<'a> fn(&'a H, F<'a, V, E>, WeexLifecycleCallback) -> Result<F<'a, V, E>, E>,
    pub all: for<'a> fn(&'a H, F<'a, V, E>, F<'a, V, E>) -> Result<F<'a, V, E>, E>,
    pub loop_callback: fn(&H, WeexLifecycleCallback) -> Result<V, E>,
    pub schedule_loop: fn(&H, V, V) -> Result<(), E>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub undefined: fn() -> V,
    pub text: fn(&str) -> V,
    pub string_units: fn(Vec<u16>) -> V,
    pub boolean: fn(bool) -> V,
    pub number: fn(f64) -> V,
    pub truthy: fn(&V) -> bool,
    pub strict_false: fn(&V) -> bool,
    pub copy: fn(&H, &V) -> Result<V, E>,
    pub template: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub numeric: fn(&H, &V) -> Result<f64, E>,
    pub equals_text: fn(&V, &str) -> bool,
    pub hash_input: fn(&H, V) -> Result<Vec<u8>, E>,
    pub error_value: fn(&E) -> V,
    pub new_error: fn(&H, V) -> E,
}
fn eager<'a, V: 'a, E: 'a>(mut f: F<'a, V, E>) -> F<'a, V, E> {
    let w = std::task::Waker::noop().clone();
    match f.as_mut().poll(&mut std::task::Context::from_waker(&w)) {
        std::task::Poll::Ready(x) => Box::pin(std::future::ready(x)),
        std::task::Poll::Pending => f,
    }
}
async fn js_await<T>(f: impl Future<Output = T>) -> T {
    let mut suspended = false;
    std::future::poll_fn(|cx| {
        if suspended {
            std::task::Poll::Ready(())
        } else {
            suspended = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
    f.await
}
fn property<H, V, E>(h: &H, r: &WeexLifecycleRuntime<H, V, E>, name: &str) -> Result<V, E> {
    (r.get)(h, &(r.this)(h), name)
}
fn method<H, V, E>(h: &H, r: &WeexLifecycleRuntime<H, V, E>, name: &'static str) -> Result<V, E> {
    (r.method)(h, &(r.this)(h), name)
}
fn call<H, V, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    name: &'static str,
    args: Vec<V>,
) -> Result<V, E> {
    (r.call)(h, method(h, r, name)?, args)
}
fn async_call<'a, H, V, E>(
    h: &'a H,
    r: &WeexLifecycleRuntime<H, V, E>,
    name: &'static str,
    args: Vec<V>,
) -> Result<F<'a, V, E>, E> {
    (r.call_async)(h, method(h, r, name)?, args)
}
fn store_method<H, V, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    name: &'static str,
) -> Result<V, E> {
    (r.method)(h, &property(h, r, "store")?, name)
}
fn disposed<H, V, E>(h: &H, r: &WeexLifecycleRuntime<H, V, E>) -> Result<bool, E> {
    Ok((r.truthy)(&property(h, r, "isDisposed")?))
}
fn public_only<H, V, E>(h: &H, r: &WeexLifecycleRuntime<H, V, E>) -> Result<bool, E> {
    Ok((r.truthy)(&property(h, r, "isPublicDataOnly")?))
}
fn loaded<H, V, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    keys: &[&'static str],
) -> Result<V, E> {
    let store = property(h, r, "store")?;
    let result = (r.copy)(h, &(r.get)(h, &store, "loaded")?)?;
    for key in keys {
        (r.define)(h, &result, key, (r.boolean)(true))?;
    }
    Ok(result)
}
fn update<H, V, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    fields: Vec<(&'static str, V)>,
    keys: &[&'static str],
) -> Result<(), E> {
    let updater = store_method(h, r, "update")?;
    let mut fields = fields;
    fields.push(("loaded", loaded(h, r, keys)?));
    (r.call)(h, updater, vec![(r.object)(fields)])?;
    Ok(())
}
fn error_message<H, V: Clone, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    error: &E,
    validation: bool,
) -> Result<V, E> {
    let value = (r.error_value)(error);
    let response = (r.optional_get)(h, &value, "response")?;
    let data = (r.optional_get)(h, &response, "data")?;
    let msg = (r.optional_get)(h, &data, "msg")?;
    if (r.truthy)(&msg) {
        return Ok(msg);
    }
    if validation {
        let response = (r.optional_get)(h, &value, "response")?;
        let data = (r.optional_get)(h, &response, "data")?;
        let message = (r.optional_get)(h, &data, "message")?;
        if (r.truthy)(&message) {
            return Ok(message);
        }
    }
    let message = (r.optional_get)(h, &value, "message")?;
    Ok(if validation && !(r.truthy)(&message) {
        (r.text)("Invalid API key or secret")
    } else {
        message
    })
}
fn report<H, V: Clone, E>(h: &H, r: &WeexLifecycleRuntime<H, V, E>, error: &E) -> Result<(), E> {
    let sink = method(h, r, "emitAndLogError")?;
    let message = error_message(h, r, error, false)?;
    (r.call)(h, sink, vec![message])?;
    Ok(())
}
fn config<'a, H, V, E>(h: &'a H, r: &WeexLifecycleRuntime<H, V, E>) -> Result<F<'a, V, E>, E> {
    let xhr = property(h, r, "xhr")?;
    let get = (r.method)(h, &xhr, "get")?;
    let endpoint = ENDPOINTS
        .iter()
        .find(|(k, _)| *k == "ACCOUNT_CONFIG")
        .unwrap()
        .1;
    (r.call_async)(h, get, vec![(r.text)(endpoint)])
}
pub struct WeexExchangeConstructionRuntime<H, V, E> {
    pub base_construct: fn(&H, V, V, V) -> Result<V, E>,
    pub client_rule: fn(&H, &V) -> Result<V, E>,
    pub api_factory: fn(&H) -> Result<V, E>,
    pub new_map: fn(&H) -> Result<V, E>,
    pub assign: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub constant: fn(&H, &'static str) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call: fn(&H, V, Vec<V>) -> Result<V, E>,
    pub construct: fn(&H, &'static str, V) -> Result<V, E>,
    pub undefined: fn() -> V,
    pub text: fn(&str) -> V,
}

pub fn construct_exchange<H, V: Clone, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    name: V,
    options: V,
    store: V,
) -> Result<V, E> {
    let construction = WeexExchangeConstructionRuntime {
        base_construct: r.base_construct,
        client_rule: r.client_rule,
        api_factory: r.api_factory,
        new_map: r.new_map,
        assign: r.assign,
        constant: r.constant,
        define: r.define,
        method: r.method,
        call: r.call,
        construct: r.construct,
        undefined: r.undefined,
        text: r.text,
    };
    construct_exchange_with_bind(h, &construction, name, options, store, r.bind)
}

pub fn construct_exchange_with_bind<H, V: Clone, E>(
    h: &H,
    r: &WeexExchangeConstructionRuntime<H, V, E>,
    name: V,
    options: V,
    store: V,
    mut bind: impl FnMut(&H, &V, &'static str) -> Result<V, E>,
) -> Result<V, E> {
    let instance = (r.base_construct)(h, name, options.clone(), store)?;
    let rule = (r.client_rule)(h, &instance)?;
    (r.define)(h, &instance, "clientOrderIdRule", rule)?;
    for field in ["xhr", "publicWebsocket", "privateWebsocket"] {
        (r.define)(h, &instance, field, (r.undefined)())?;
    }
    (r.define)(
        h,
        &instance,
        "grafanaEndpointsEvery",
        (r.constant)(h, "GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    (r.define)(
        h,
        &instance,
        "grafanaEndpointsPeriodic",
        (r.constant)(h, "GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    let settings = (r.new_map)(h)?;
    (r.define)(h, &instance, "symbolSettings", settings)?;
    (r.define)(h, &instance, "accountPositionMode", (r.text)("oneway"))?;
    for field in [
        "validateAccount",
        "getAccount",
        "start",
        "tick",
        "fetchMarkets",
        "fetchTickers",
        "fetchSymbolMaxLeverage",
        "fetchLeverageForSymbol",
        "updateSymbolMetadata",
        "fetchBalance",
        "fetchBalanceAndPositions",
        "fetchPositions",
        "fetchOrders",
        "fetchNormalOrders",
        "fetchAlgoOrders",
        "fetchOHLCV",
        "listenOHLCV",
        "listenOrderBook",
        "fetchOrderBook",
        "fetchPublicTrades",
        "fetchHistoricalFills",
        "fetchIncomeHistory",
        "fetchIncomeHistoryRange",
        "placeOrder",
        "placeOrders",
        "updateOrder",
        "cancelOrders",
        "cancelSymbolOrders",
        "cancelAllOrders",
        "setLeverage",
        "changeMarginType",
        "changePositionMode",
        "nukePositions",
        "nuke",
        "getSymbolConfigFromCache",
        "getOrFetchSymbolConfig",
        "fetchSymbolConfigs",
        "fetchOpenOrderRows",
        "fetchUserTradeRows",
        "parseOrderResult",
        "isBatchableRegularOrder",
        "placeRegularOrderBatch",
        "isAlgoOrder",
        "inferPositionSide",
        "formatOrderPayload",
    ] {
        let bound = bind(h, &instance, field)?;
        (r.define)(h, &instance, field, bound)?;
    }
    let factory = (r.method)(h, &instance, "createXhrWithSecondaryKey")?;
    let create = (r.api_factory)(h)?;
    let xhr = (r.call)(h, factory, vec![options, create])?;
    (r.assign)(h, &instance, "xhr", xhr)?;
    let public = (r.construct)(h, "WeexPublicWebsocket", instance.clone())?;
    (r.assign)(h, &instance, "publicWebsocket", public)?;
    let private = (r.construct)(h, "WeexPrivateWebsocket", instance.clone())?;
    (r.assign)(h, &instance, "privateWebsocket", private)?;
    Ok(instance)
}
pub fn lifecycle_callback<H, V: Clone, E>(
    h: &H,
    r: &WeexLifecycleRuntime<H, V, E>,
    callback: WeexLifecycleCallback,
    value: V,
) -> Result<V, E> {
    match callback {
        WeexLifecycleCallback::ValidationBalance => (r.get)(h, &value, "balance"),
        WeexLifecycleCallback::StartBalancePositions => {
            let balance = (r.get)(h, &value, "balance")?;
            let positions = (r.get)(h, &value, "positions")?;
            if !disposed(h, r)? {
                let updater = store_method(h, r, "update")?;
                let loaded = loaded(h, r, &["balance", "positions"])?;
                (r.call)(
                    h,
                    updater,
                    vec![(r.object)(vec![
                        ("balance", balance),
                        ("positions", positions),
                        ("loaded", loaded),
                    ])],
                )?;
            }
            Ok((r.undefined)())
        }
        WeexLifecycleCallback::StartOrders => {
            if !disposed(h, r)? {
                update(h, r, vec![("orders", value)], &["orders"])?;
            }
            Ok((r.undefined)())
        }
        WeexLifecycleCallback::Tick => call(h, r, "tick", vec![]),
    }
}
pub fn validate_account<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexLifecycleRuntime<H, V, E>,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let outcome: Result<V, E> = async {
            let response = js_await(config(h, r)?).await?;
            let data = (r.get)(h, &response, "data")?;
            if (r.strict_false)(&(r.optional_get)(h, &data, "canTrade")?) {
                return Ok((r.object)(vec![(
                    "error",
                    (r.text)("WEEX API key is valid but trading is disabled"),
                )]));
            }
            let future = async_call(h, r, "fetchBalanceAndPositions", vec![])?;
            let future = (r.then)(h, future, WeexLifecycleCallback::ValidationBalance)?;
            let balance = match js_await(future).await {
                Ok(value) => value,
                Err(_) => (r.undefined)(),
            };
            Ok((r.object)(vec![
                ("error", (r.text)("")),
                ("balance", balance),
            ]))
        }
        .await;
        match outcome {
            Ok(x) => Ok(x),
            Err(error) => {
                let message = error_message(h, r, &error, true)?;
                call(h, r, "emitAndLogError", vec![message.clone()])?;
                Ok((r.object)(vec![("error", message)]))
            }
        }
    }))
}
pub fn get_account<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexLifecycleRuntime<H, V, E>,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let validation = js_await(async_call(h, r, "validateAccount", vec![])?).await?;
        if (r.truthy)(&(r.get)(h, &validation, "error")?) {
            return Err((r.new_error)(h, (r.get)(h, &validation, "error")?));
        }
        let options = property(h, r, "options")?;
        let key = (r.get)(h, &options, "key")?;
        let key = if (r.truthy)(&key) {
            key
        } else {
            (r.text)("weex")
        };
        let input = (r.hash_input)(h, key)?;
        let hash = Sha256::digest(&input);
        let mut hex = String::new();
        use std::fmt::Write;
        for byte in hash.iter().take(12) {
            write!(&mut hex, "{byte:02x}").unwrap();
        }
        Ok((r.object)(vec![(
            "userId",
            (r.text)(&format!("weex:{hex}")),
        )]))
    }))
}
pub fn start<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexLifecycleRuntime<H, V, E>,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let external = js_await(async_call(h, r, "initExternalPublicData", vec![])?).await?;
        if !(r.truthy)(&external) {
            let markets = js_await(async_call(h, r, "fetchMarkets", vec![])?).await?;
            if disposed(h, r)? {
                return Ok((r.undefined)());
            }
            update(h, r, vec![("markets", markets.clone())], &["markets"])?;
            let tickers = js_await(async_call(h, r, "fetchTickers", vec![])?).await?;
            if disposed(h, r)? {
                return Ok((r.undefined)());
            }
            update(h, r, vec![("tickers", tickers.clone())], &["tickers"])?;
            let socket = property(h, r, "publicWebsocket")?;
            let connect = (r.method)(h, &socket, "connectAndSubscribe")?;
            (r.call)(h, connect, vec![])?;
            let log = method(h, r, "log")?;
            let m = (r.get)(h, &markets, "length")?;
            let t = (r.get)(h, &tickers, "length")?;
            let m = (r.numeric)(h, &m)?;
            let t = (r.numeric)(h, &t)?;
            let n = (r.number)(safe_cex_num::math_min(m, t));
            let mut msg: Vec<u16> = "Loaded ".encode_utf16().collect();
            msg.extend((r.template)(h, &n)?);
            msg.extend(" WEEX markets".encode_utf16());
            (r.call)(h, log, vec![(r.string_units)(msg)])?;
        }
        if public_only(h, r)? {
            update(h, r, vec![], &["balance", "positions", "orders"])?;
            return Ok((r.undefined)());
        }
        let socket = property(h, r, "privateWebsocket")?;
        let connect = (r.method)(h, &socket, "connectAndSubscribe")?;
        (r.call)(h, connect, vec![])?;
        let configured: Result<(), E> = async {
            let response = js_await(config(h, r)?).await?;
            let data = (r.get)(h, &response, "data")?;
            let mode = if (r.truthy)(&(r.optional_get)(h, &data, "dualSidePosition")?) {
                "hedged"
            } else {
                "oneway"
            };
            (r.assign)(h, &(r.this)(h), "accountPositionMode", (r.text)(mode))?;
            let setter = store_method(h, r, "setSetting")?;
            let mode = property(h, r, "accountPositionMode")?;
            let hedged = (r.equals_text)(&mode, "hedged");
            (r.call)(h, setter, vec![(r.text)("isHedged"), (r.boolean)(hedged)])?;
            Ok(())
        }
        .await;
        if let Err(error) = configured {
            let log = method(h, r, "log")?;
            let message = error_message(h, r, &error, false)?;
            let mut text: Vec<u16> = "Failed to load WEEX account config: "
                .encode_utf16()
                .collect();
            text.extend((r.template)(h, &message)?);
            (r.call)(h, log, vec![(r.string_units)(text), (r.text)("warning")])?;
        }
        js_await(async_call(h, r, "fetchSymbolConfigs", vec![])?).await?;
        if disposed(h, r)? {
            return Ok((r.undefined)());
        }
        let balance = async_call(h, r, "fetchBalanceAndPositions", vec![])?;
        let balance = (r.then)(h, balance, WeexLifecycleCallback::StartBalancePositions)?;
        let orders = async_call(h, r, "fetchOrders", vec![])?;
        let orders = (r.then)(h, orders, WeexLifecycleCallback::StartOrders)?;
        js_await((r.all)(h, balance, orders)?).await?;
        js_await(async_call(h, r, "tick", vec![])?).await?;
        call(h, r, "log", vec![(r.text)("Ready to trade on WEEX")])?;
        Ok((r.undefined)())
    }))
}
pub fn tick<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexLifecycleRuntime<H, V, E>,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        if disposed(h, r)? || public_only(h, r)? {
            return Ok((r.undefined)());
        }
        let balance: Result<bool, E> = async {
            let result = js_await(async_call(h, r, "fetchBalanceAndPositions", vec![])?).await?;
            let balance = (r.get)(h, &result, "balance")?;
            let positions = (r.get)(h, &result, "positions")?;
            if disposed(h, r)? {
                return Ok(true);
            }
            let add = store_method(h, r, "addOrUpdatePositionsFull")?;
            (r.call)(h, add, vec![positions])?;
            update(h, r, vec![("balance", balance)], &["balance", "positions"])?;
            Ok(false)
        }
        .await;
        match balance {
            Err(error) => report(h, r, &error)?,
            Ok(true) => return Ok((r.undefined)()),
            Ok(false) => {}
        }
        let orders: Result<bool, E> = async {
            let orders = js_await(async_call(h, r, "fetchOrders", vec![])?).await?;
            if disposed(h, r)? {
                return Ok(true);
            }
            update(h, r, vec![("orders", orders)], &["orders"])?;
            Ok(false)
        }
        .await;
        match orders {
            Err(error) => report(h, r, &error)?,
            Ok(true) => return Ok((r.undefined)()),
            Ok(false) => {}
        }
        if !disposed(h, r)? {
            let callback = (r.loop_callback)(h, WeexLifecycleCallback::Tick)?;
            let interval = call(h, r, "getTickInterval", vec![])?;
            (r.schedule_loop)(h, callback, interval)?;
        }
        Ok((r.undefined)())
    }))
}
