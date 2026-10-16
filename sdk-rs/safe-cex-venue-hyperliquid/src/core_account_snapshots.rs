//! Core account bootstrap, snapshots and private position-query leaves.
//! Immediate first-poll bodies; runtime/primitive/virtual calls are explicit bindings.
use crate::{
    private_message_dispatch::{DispatchOperator, PrivateMessageHost},
    spot_json::JsString,
};
use std::future::Future;
#[derive(Clone, Debug)]
pub enum SnapshotPredicate<V> {
    SpotBalance,
    PositionSymbol { symbols: V },
}
pub trait CoreSnapshotHost: PrivateMessageHost {
    fn null_value(&self) -> Self::Value;
    fn add_values(&mut self, a: &Self::Value, b: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn return_await_value(
        &mut self,
        value: Self::Value,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn await_value(
        &mut self,
        value: Self::Value,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn construct_error(
        &mut self,
        constructor: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Error, Self::Error>;
    fn instance_of(
        &mut self,
        value: &Self::Error,
        constructor: &Self::Value,
    ) -> Result<bool, Self::Error>;
    fn snapshot_predicate(
        &mut self,
        capture: SnapshotPredicate<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn array_define(
        &mut self,
        array: &Self::Value,
        index: usize,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn spread_into(
        &mut self,
        object: &Self::Value,
        source: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn iterator(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, iterator: &Self::Value);
    fn close_after_return(&mut self, iterator: &Self::Value) -> Result<(), Self::Error>;
    fn pair(&mut self, value: &Self::Value) -> Result<[Self::Value; 2], Self::Error>;
}
fn lit<H: PrivateMessageHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn optional<H: PrivateMessageHost>(h: &mut H, v: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, n)
    }
}
fn path<H: PrivateMessageHost>(h: &mut H, v: &H::Value, ns: &[&str]) -> Result<H::Value, H::Error> {
    let mut v = v.clone();
    for n in ns {
        v = optional(h, &v, n)?;
    }
    Ok(v)
}
fn method<H: PrivateMessageHost>(
    h: &mut H,
    v: &H::Value,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(v, n)?;
    h.call(&f, v, args)
}
fn options<H: PrivateMessageHost>(h: &mut H, e: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    let opts = h.get(e, "options")?;
    h.get(&opts, n)
}
fn user<H: PrivateMessageHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let vault = options(h, e, "vault")?;
    if h.truthy(&vault) {
        Ok(vault)
    } else {
        options(h, e, "applicationId")
    }
}
fn free<H: PrivateMessageHost>(
    h: &mut H,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let u = h.undefined();
    h.call(&f, &u, args)
}
fn define_text<H: PrivateMessageHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    s: &str,
) -> Result<(), H::Error> {
    let v = lit(h, s);
    h.define_data(o, n, &v)
}
fn headers<H: PrivateMessageHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let config = h.object()?;
    let headers = h.object()?;
    define_text(h, &headers, "Content-Type", "application/json")?;
    h.define_data(&config, "headers", &headers)?;
    Ok(config)
}
fn endpoint<H: PrivateMessageHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let g = h.global("ENDPOINTS")?;
    h.get(&g, "INFO")
}
fn severity<H: PrivateMessageHost>(h: &mut H, n: &str) -> Result<H::Value, H::Error> {
    let g = h.global("LogSeverity")?;
    h.get(&g, n)
}
fn log<H: PrivateMessageHost>(
    h: &mut H,
    e: &H::Value,
    message: &str,
    n: &str,
) -> Result<(), H::Error> {
    let f = h.get(e, "log")?;
    let msg = lit(h, message);
    let sev = severity(h, n)?;
    h.call(&f, e, &[msg, sev])?;
    Ok(())
}
fn error_log<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    prefix: &str,
    error: &H::Error,
    n: &str,
) -> Result<(), H::Error> {
    let f = h.get(e, "log")?;
    let v = h.error_value(error);
    let mut s = prefix.encode_utf16().collect::<Vec<_>>();
    s.extend(h.template_string(&v)?.0);
    let msg = h.literal(&JsString(s));
    let sev = severity(h, n)?;
    h.call(&f, e, &[msg, sev])?;
    Ok(())
}
pub fn run_snapshot_predicate<H: CoreSnapshotHost>(
    h: &mut H,
    capture: &SnapshotPredicate<H::Value>,
    entry: &H::Value,
) -> Result<H::Value, H::Error> {
    match capture {
        SnapshotPredicate::SpotBalance => {
            let coin = optional(h, entry, "coin")?;
            if h.typeof_value(&coin) != "string" {
                return Ok(h.boolean(false));
            }
            let coin = h.get(entry, "coin")?;
            let length = h.get(&coin, "length")?;
            let zero = h.number(0.0);
            h.operator(DispatchOperator::Greater, &length, &zero)
        }
        SnapshotPredicate::PositionSymbol { symbols } => {
            let f = h.get(symbols, "includes")?;
            let symbol = h.get(entry, "symbol")?;
            h.call(&f, symbols, &[symbol])
        }
    }
}
pub fn handle_spot_clearinghouse_state_update<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    state: &H::Value,
    source: &H::Value,
) -> Result<H::Value, H::Error> {
    let source = if h.strict_equal(source, &h.undefined()) {
        lit(h, "legacy")
    } else {
        source.clone()
    };
    let track = method(h, e, "shouldTrackSpotHoldings", &[])?;
    if !h.truthy(&track) {
        return Ok(h.undefined());
    }
    let array = h.global("Array")?;
    let f = h.get(&array, "isArray")?;
    let balances = optional(h, state, "balances")?;
    let yes = h.call(&f, &array, &[balances])?;
    if !h.truthy(&yes) {
        return Ok(h.undefined());
    }
    if h.strict_equal(&source, &lit(h, "rest")) {
        let authoritative = h.get(e, "hasAuthoritativeUnifiedSpotWsSnapshot")?;
        if h.truthy(&authoritative) {
            return Ok(h.undefined());
        }
    }
    let yes = h.boolean(true);
    h.set(e, "hasSpotCollateralSnapshot", &yes)?;
    let cached = h.object()?;
    let balances = h.get(state, "balances")?;
    let f = h.get(&balances, "filter")?;
    let predicate = h.snapshot_predicate(SnapshotPredicate::SpotBalance)?;
    let balances = h.call(&f, &balances, &[predicate])?;
    h.define_data(&cached, "balances", &balances)?;
    let maintenance = h.get(state, "tokenToAvailableAfterMaintenance")?;
    h.define_data(&cached, "tokenToAvailableAfterMaintenance", &maintenance)?;
    h.set(e, "cachedSpotCollateralState", &cached)?;
    let available = method(
        h,
        e,
        "getAuthoritativeUnifiedUsdcAvailability",
        &[state.clone()],
    )?;
    if !h.is_null(&available) {
        if h.strict_equal(&source, &lit(h, "websocket")) {
            let yes = h.boolean(true);
            h.set(e, "hasAuthoritativeUnifiedSpotWsSnapshot", &yes)?;
        }
        let f = h.get(e, "buildUnifiedSpotCollateralSnapshot")?;
        let cached = h.get(e, "cachedSpotCollateralState")?;
        let balances = h.get(&cached, "balances")?;
        let snapshot = h.call(&f, e, &[balances])?;
        let result = h.object()?;
        let total = h.get(&snapshot, "total")?;
        let upnl = h.get(e, "latestCorePerpsUpnl")?;
        let total = h.operator(DispatchOperator::Subtract, &total, &upnl)?;
        h.define_data(&result, "total", &total)?;
        let total = h.get(&snapshot, "total")?;
        let used = h.operator(DispatchOperator::Subtract, &total, &available)?;
        h.define_data(&result, "used", &used)?;
        h.define_data(&result, "free", &available)?;
        h.set(e, "latestAuthoritativeUnifiedBalanceSnapshot", &result)?;
    }
    method(h, e, "applyBalanceFromCurrentMode", &[])?;
    Ok(h.undefined())
}
pub async fn validate_account<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let result = (|| {
        let f = h.global("validateWalletAndPrivateKey")?;
        let secret = options(h, e, "secret")?;
        let key = options(h, e, "key")?;
        let u = h.undefined();
        let valid = h.call(&f, &u, &[secret, key])?;
        if !h.truthy(&valid) {
            let constructor = h.global("Error")?;
            let message = lit(h, "Invalid private key & wallet address pair");
            return Err(h.construct_error(&constructor, &[message])?);
        }
        let result = h.object()?;
        define_text(h, &result, "error", "")?;
        Ok(result)
    })();
    match result {
        Ok(v) => Ok(v),
        Err(error) => {
            let err = h.error_value(&error);
            let f = h.get(e, "emitAndLogError")?;
            let mut message = path(h, &err, &["response", "data", "message"])?;
            if !h.truthy(&message) {
                message = optional(h, &err, "message")?;
            }
            h.call(&f, e, &[message])?;
            let result = h.object()?;
            let mut message = path(h, &err, &["response", "data", "msg"])?;
            if !h.truthy(&message) {
                message = optional(h, &err, "message")?;
            }
            if !h.truthy(&message) {
                message = lit(h, "Invalid API key or secret");
            }
            h.define_data(&result, "error", &message)?;
            Ok(result)
        }
    }
}
pub async fn fetch_builder_fee_approval<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let app = options(h, e, "applicationId")?;
    if !h.truthy(&app) {
        let no = h.boolean(false);
        h.set(e, "_builderFeeApproved", &no)?;
        let result = h.object()?;
        h.define_data(&result, "approved", &no)?;
        define_text(h, &result, "error", "missing applicationId")?;
        h.set(e, "_builderFeeLastCheck", &result)?;
        return Ok(h.undefined());
    }
    let outcome = async {
        let f = h.global("resolveHyperliquidBuilder")?;
        let opts = h.get(e, "options")?;
        let u = h.undefined();
        let builder = h.call(&f, &u, &[opts])?;
        let timeout = h.get(e, "withStartupTimeout")?;
        let checker = h.global("checkBuilderFeeApproved")?;
        let app = options(h, e, "applicationId")?;
        let address = h.get(&builder, "address")?;
        let mut testnet = options(h, e, "testnet")?;
        if h.is_nullish(&testnet) {
            testnet = h.boolean(false);
        }
        let fee = h.get(&builder, "feeTenthsBps")?;
        let promise = h.call(&checker, &u, &[app, address, testnet, fee])?;
        let class = h.global("HyperliquidExchange")?;
        let ms = h.get(&class, "BUILDER_FEE_APPROVAL_TIMEOUT_MS")?;
        let label = lit(h, "Builder fee approval check");
        let fallback = h.object()?;
        let no = h.boolean(false);
        h.define_data(&fallback, "approved", &no)?;
        let result = h.call(&timeout, e, &[promise, ms, label, fallback])?;
        let result = h.await_value(result).await?;
        let approved = h.get(&result, "approved")?;
        h.set(e, "_builderFeeApproved", &approved)?;
        h.set(e, "_builderFeeLastCheck", &result)?;
        let logger = h.get(e, "log")?;
        let approved = h.get(&result, "approved")?;
        let text = if h.truthy(&approved) {
            "approved"
        } else {
            "not approved"
        };
        let text = lit(h, text);
        let mut message = "Builder fee approval status: "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend(h.template_string(&text)?.0);
        let message = h.literal(&JsString(message));
        let approved = h.get(&result, "approved")?;
        let sev = severity(h, if h.truthy(&approved) { "Info" } else { "Debug" })?;
        h.call(&logger, e, &[message, sev])?;
        Ok::<(), H::Error>(())
    }
    .await;
    if let Err(error) = outcome {
        error_log(
            h,
            e,
            "Failed to check builder fee approval: ",
            &error,
            "Warning",
        )?;
        let null = h.null_value();
        h.set(e, "_builderFeeApproved", &null)?;
        let last = h.object()?;
        let no = h.boolean(false);
        h.define_data(&last, "approved", &no)?;
        let constructor = h.global("Error")?;
        let message = if h.instance_of(&error, &constructor)? {
            let v = h.error_value(&error);
            h.get(&v, "message")?
        } else {
            let f = h.global("String")?;
            let v = h.error_value(&error);
            let u = h.undefined();
            h.call(&f, &u, &[v])?
        };
        h.define_data(&last, "error", &message)?;
        h.set(e, "_builderFeeLastCheck", &last)?;
    }
    Ok(h.undefined())
}
fn log_balance_summary<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    let summary = method(h, e, "extractClearinghouseBalanceSummary", &[data.clone()])?;
    if !h.truthy(&summary) {
        let f = h.get(e, "log")?;
        let b = h.global("Boolean")?;
        let margin = optional(h, data, "marginSummary")?;
        let u = h.undefined();
        let margin = h.call(&b, &u, &[margin])?;
        let mut message = "No usable balance summary. marginSummary="
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend(h.template_string(&margin)?.0);
        message.extend(" crossMarginSummary=".encode_utf16());
        let b = h.global("Boolean")?;
        let cross = optional(h, data, "crossMarginSummary")?;
        let cross = h.call(&b, &u, &[cross])?;
        message.extend(h.template_string(&cross)?.0);
        let msg = h.literal(&JsString(message));
        let sev = severity(h, "Warning")?;
        h.call(&f, e, &[msg, sev])?;
        return Ok(());
    }
    let used = h.get(&summary, "totalMarginUsed")?;
    let total = h.get(&summary, "accountValue")?;
    let free = h.operator(DispatchOperator::Subtract, &total, &used)?;
    let f = h.get(e, "log")?;
    let source = h.get(&summary, "sourceField")?;
    let mut message = "Core-perp account state fetched ("
        .encode_utf16()
        .collect::<Vec<_>>();
    message.extend(h.template_string(&source)?.0);
    message.extend("): total=".encode_utf16());
    let total = h.get(&summary, "accountValue")?;
    let two = h.number(2.0);
    let total = method(h, &total, "toFixed", &[two.clone()])?;
    message.extend(h.template_string(&total)?.0);
    message.extend(", used=".encode_utf16());
    let used = method(h, &used, "toFixed", &[two.clone()])?;
    message.extend(h.template_string(&used)?.0);
    message.extend(", free=".encode_utf16());
    let free = method(h, &free, "toFixed", &[two])?;
    message.extend(h.template_string(&free)?.0);
    let msg = h.literal(&JsString(message));
    let sev = severity(h, "Info")?;
    h.call(&f, e, &[msg, sev])?;
    Ok(())
}
pub async fn fetch_core_perps_state<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let user = user(h, e)?;
    if !h.truthy(&user) {
        log(
            h,
            e,
            "Skipping core-perp account-state fetch: no applicationId/vault configured",
            "Warning",
        )?;
        return Ok(h.null_value());
    }
    let body = h.object()?;
    let info = h.global("INFO_TYPES")?;
    let ty = h.get(&info, "PERPS_CLEARINGHOUSE_STATE")?;
    h.define_data(&body, "type", &ty)?;
    h.define_data(&body, "user", &user)?;
    let config = headers(h)?;
    let outcome = async {
        log(
            h,
            e,
            "Fetching core-perp account state via REST API",
            "Debug",
        )?;
        let xhr = h.get(e, "infoXhr")?;
        let post = h.get(&xhr, "post")?;
        let endpoint = endpoint(h)?;
        let promise = h.call(&post, &xhr, &[endpoint, body, config])?;
        let response = h.await_value(promise).await?;
        let data = h.get(&response, "data")?;
        log_balance_summary(h, e, &data)?;
        Ok::<_, H::Error>(data)
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(error) => {
            error_log(
                h,
                e,
                "Core-perp account-state fetch failed: ",
                &error,
                "Warning",
            )?;
            Ok(h.null_value())
        }
    }
}
pub fn warn_on_mismatched_api_wallet<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    explicit: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(explicit) {
        return Ok(h.undefined());
    }
    let opts = h.get(e, "options")?;
    let app = h.get(&opts, "applicationId")?;
    let key = h.get(&opts, "key")?;
    let secret = h.get(&opts, "secret")?;
    let vault = h.get(&opts, "vault")?;
    let wallet = if h.truthy(&key) {
        key
    } else if h.truthy(&vault) {
        vault
    } else {
        app
    };
    if !h.truthy(&wallet) || !h.truthy(&secret) {
        return Ok(h.undefined());
    }
    let result = free(h, "validateWalletAndPrivateKey", &[secret, wallet.clone()]);
    if let Ok(v) = result {
        if h.truthy(&v) {
            return Ok(h.undefined());
        }
    }
    let f = h.get(e, "emitAndLogError")?;
    let mut message =
        "Hyperliquid credentials are invalid: the saved private key does not match API wallet "
            .encode_utf16()
            .collect::<Vec<_>>();
    message.extend(h.template_string(&wallet)?.0);
    message.extend(". ".encode_utf16());
    let left = h.literal(&JsString(message));
    let right = lit(h, "Orders will be rejected until the account is re-added.");
    let message = h.add_values(&left, &right)?;
    h.call(&f, e, &[message])?;
    Ok(h.undefined())
}
pub async fn fetch_and_set_private_snapshot<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    _tries: &H::Value,
) -> Result<H::Value, H::Error> {
    let public = h.get(e, "isPublicDataOnly")?;
    if h.truthy(&public) {
        return Ok(h.undefined());
    }
    let disposed = h.get(e, "isDisposed")?;
    if h.truthy(&disposed) {
        return Ok(h.undefined());
    }
    let promise = h.global("Promise")?;
    let all = h.get(&promise, "all")?;
    let values = h.array()?;
    let orders = method(h, e, "fetchOrders", &[])?;
    h.array_define(&values, 0, &orders)?;
    let core = method(h, e, "fetchCorePerpsState", &[])?;
    h.array_define(&values, 1, &core)?;
    let value = h.call(&all, &promise, &[values])?;
    let result = h.await_value(value).await?;
    let [orders, core] = h.pair(&result)?;
    let disposed = h.get(e, "isDisposed")?;
    if h.truthy(&disposed) {
        return Ok(h.undefined());
    }
    let store = h.get(e, "store")?;
    let update = h.get(&store, "update")?;
    let patch = h.object()?;
    h.define_data(&patch, "orders", &orders)?;
    let loaded = h.object()?;
    let loaded_store = h.get(e, "store")?;
    let old = h.get(&loaded_store, "loaded")?;
    h.spread_into(&loaded, &old)?;
    let yes = h.boolean(true);
    h.define_data(&loaded, "orders", &yes)?;
    h.define_data(&patch, "loaded", &loaded)?;
    h.call(&update, &store, &[patch])?;
    if h.truthy(&core) {
        let ws = h.get(e, "privateWebsocket")?;
        let yes = h.boolean(true);
        method(h, &ws, "applyCoreClearinghouseState", &[core, yes])?;
    }
    Ok(h.undefined())
}
fn default_fees<H: CoreSnapshotHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let result = h.object()?;
    let taker = h.number(0.00045);
    h.define_data(&result, "takerFee", &taker)?;
    let maker = h.number(0.00015);
    h.define_data(&result, "makerFee", &maker)?;
    Ok(result)
}
pub async fn fetch_fee<H: CoreSnapshotHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let outcome = async {
        let xhr = h.get(e, "infoXhr")?;
        let post = h.get(&xhr, "post")?;
        let endpoint = endpoint(h)?;
        let body = h.object()?;
        define_text(h, &body, "type", "userFees")?;
        let user = options(h, e, "key")?;
        h.define_data(&body, "user", &user)?;
        let config = headers(h)?;
        let value = h.call(&post, &xhr, &[endpoint, body, config])?;
        let value = h.await_value(value).await?;
        let data = h.get(&value, "data")?;
        if !h.truthy(&data) {
            return default_fees(h);
        }
        let mut rates = vec![];
        for (names, default) in [
            (["userCrossRate", "crossRate"], "0.00045"),
            (["userAddRate", "addRate"], "0.00015"),
        ] {
            let parser = h.global("parseFloat")?;
            let mut v = h.get(&data, names[0])?;
            if !h.truthy(&v) {
                v = h.get(&data, names[1])?;
            }
            if !h.truthy(&v) {
                v = lit(h, default);
            }
            let u = h.undefined();
            rates.push(h.call(&parser, &u, &[v])?);
        }
        let result = h.object()?;
        h.define_data(&result, "takerFee", &rates[0])?;
        h.define_data(&result, "makerFee", &rates[1])?;
        Ok::<_, H::Error>(result)
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(error) => {
            error_log(
                h,
                e,
                "Failed to fetch user fees, using defaults: ",
                &error,
                "Warning",
            )?;
            default_fees(h)
        }
    }
}
pub async fn fetch_positions<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    symbols: &H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(e, "store")?;
    let positions = h.get(&store, "positions")?;
    let length = h.get(&positions, "length")?;
    let zero = h.number(0.0);
    let has = h.operator(DispatchOperator::Greater, &length, &zero)?;
    if h.truthy(&has) {
        let copy = h.array()?;
        let store = h.get(e, "store")?;
        let positions = h.get(&store, "positions")?;
        let iterator = h.iterator(&positions)?;
        let mut i = 0;
        while let Some(v) = h.next(&iterator)? {
            if let Err(error) = h.array_define(&copy, i, &v) {
                h.close_after_throw(&iterator);
                return Err(error);
            }
            i += 1;
        }
        return Ok(copy);
    }
    let positions = method(h, e, "getFakePositions", &[])?;
    if !h.truthy(symbols) {
        return Ok(positions);
    }
    let length = h.get(symbols, "length")?;
    if h.strict_equal(&length, &h.number(0.0)) {
        return Ok(positions);
    }
    let f = h.get(&positions, "filter")?;
    let callback = h.snapshot_predicate(SnapshotPredicate::PositionSymbol {
        symbols: symbols.clone(),
    })?;
    h.call(&f, &positions, &[callback])
}
pub async fn fetch_all_hip3_positions<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let dexes = h.get(e, "hip3DexNames")?;
    let length = h.get(&dexes, "length")?;
    if h.strict_equal(&length, &h.number(0.0)) {
        return Ok(h.undefined());
    }
    let dexes = h.get(e, "hip3DexNames")?;
    let iterator = h.iterator(&dexes)?;
    while let Some(dex) = h.next(&iterator)? {
        let disposed = match h.get(e, "isDisposed") {
            Ok(v) => v,
            Err(error) => {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        };
        if h.truthy(&disposed) {
            h.close_after_return(&iterator)?;
            return Ok(h.undefined());
        }
        let _caught = async {
            let value = method(h, e, "fetchHip3Positions", &[dex])?;
            h.await_value(value).await?;
            Ok::<(), H::Error>(())
        }
        .await;
    }
    Ok(h.undefined())
}
pub async fn fetch_historical_positions<H: CoreSnapshotHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.get(e, "foldClosedPositionRecords")?;
    let scope = h.object()?;
    let yes = h.boolean(true);
    h.define_data(&scope, "symbolScoped", &yes)?;
    let value = h.call(&f, e, &[opts.clone(), scope])?;
    h.return_await_value(value).await
}
