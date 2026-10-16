//! Order-request, margin-action and diagnostic leaves. Imported helpers and JS
//! primitive implementations are supplied bindings; Rust owns these source bodies.
use crate::{
    balance_state::{BalanceHost, BalanceOperator},
    spot_json::JsString,
};
use std::future::Future;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestComparison {
    Less,
    GreaterEqual,
}
pub trait OrderRequestHost: BalanceHost {
    /// Await PromiseResolve/thenable assimilation, suspension and rejection supplied
    /// by the embedding runtime. The caller immediately first-polls the body;
    /// arbitrary eager JS-call vs lazy Rust-future scheduling is outside this leaf.
    fn await_value(
        &mut self,
        value: Self::Value,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
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
    fn type_is(&self, value: &Self::Value, kind: &str) -> bool;
    fn unary_number(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn compare(
        &mut self,
        op: RequestComparison,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: OrderRequestHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: OrderRequestHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    a: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(o, n)?;
    h.call(&f, o, a)
}
fn free<H: OrderRequestHost>(h: &mut H, n: &str, a: &[H::Value]) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let u = h.undefined();
    h.call(&f, &u, a)
}
fn optional<H: OrderRequestHost>(h: &mut H, o: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(o) {
        Ok(h.undefined())
    } else {
        h.get(o, n)
    }
}
fn options<H: OrderRequestHost>(h: &mut H, e: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    let o = h.get(e, "options")?;
    h.get(&o, n)
}
fn define_text<H: OrderRequestHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    s: &str,
) -> Result<(), H::Error> {
    let v = lit(h, s);
    h.define_data(o, n, &v)
}
fn static_error<H: OrderRequestHost>(h: &mut H, s: &str) -> Result<H::Error, H::Error> {
    let c = h.global("Error")?;
    let v = lit(h, s);
    h.construct_error(&c, &[v])
}
fn headers<H: OrderRequestHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let c = h.object()?;
    let hdr = h.object()?;
    define_text(h, &hdr, "Content-Type", "application/json")?;
    h.define_data(&c, "headers", &hdr)?;
    Ok(c)
}
fn exchange_endpoint<H: OrderRequestHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let e = h.global("ENDPOINTS")?;
    h.get(&e, "EXCHANGE")
}
fn clock<H: OrderRequestHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let c = h.global("virtualClock")?;
    method(h, &c, "getCurrentTimeMs", &[])
}

pub async fn get_builder_info<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let application = options(h, e, "applicationId")?;
    if !h.truthy(&application) {
        return Ok(h.undefined());
    }
    let approved = h.get(e, "_builderFeeApproved")?;
    let null = h.null();
    if h.strict_equal(&approved, &null) {
        let p = method(h, e, "fetchBuilderFeeApproval", &[])?;
        h.await_value(p).await?;
    }
    let approved = h.get(e, "_builderFeeApproved")?;
    if h.truthy(&approved) {
        let resolve = h.global("resolveHyperliquidBuilder")?;
        let opts = h.get(e, "options")?;
        let u = h.undefined();
        let builder = h.call(&resolve, &u, &[opts])?;
        let result = h.object()?;
        let address = h.get(&builder, "address")?;
        let lower = method(h, &address, "toLowerCase", &[])?;
        h.define_data(&result, "b", &lower)?;
        let fee = h.get(&builder, "feeTenthsBps")?;
        h.define_data(&result, "f", &fee)?;
        return Ok(result);
    }
    Ok(h.undefined())
}
// Re-read each source member in the &&/typeof/conditional expression.
fn truthy_object_member<H: OrderRequestHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
) -> Result<H::Value, H::Error> {
    let first = optional(h, o, n)?;
    if h.truthy(&first) {
        let second = h.get(o, n)?;
        if h.type_is(&second, "object") {
            return h.get(o, n);
        }
    }
    Ok(h.undefined())
}
fn string_member<H: OrderRequestHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
) -> Result<H::Value, H::Error> {
    let first = optional(h, o, n)?;
    if h.type_is(&first, "string") {
        h.get(o, n)
    } else {
        Ok(h.undefined())
    }
}
fn yes_no<H: OrderRequestHost>(h: &H, v: &H::Value) -> H::Value {
    lit(h, if h.truthy(v) { "yes" } else { "no" })
}
fn masked<H: OrderRequestHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    optional_root: bool,
) -> Result<H::Value, H::Error> {
    let mask = h.global("maskHyperliquidAddress")?;
    let v = if optional_root {
        optional(h, o, n)?
    } else {
        h.get(o, n)?
    };
    let u = h.undefined();
    h.call(&mask, &u, &[v])
}
pub fn get_order_error_telemetry<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    data: &H::Value,
    message: &H::Value,
) -> Result<H::Value, H::Error> {
    let lower = method(h, message, "toLowerCase", &[])?;
    let keyword = lit(h, "builder fee");
    let includes = method(h, &lower, "includes", &[keyword])?;
    if !h.truthy(&includes) {
        return Ok(h.undefined());
    }
    let payload = free(h, "parseHyperliquidExchangePayload", &[data.clone()])?;
    let action = truthy_object_member(h, &payload, "action")?;
    let builder = truthy_object_member(h, &action, "builder")?;
    let last = h.get(e, "_builderFeeLastCheck")?;
    let max = optional(h, &last, "maxFeeRateTenthsBps")?;
    let resolve = h.global("resolveHyperliquidBuilder")?;
    let opts = h.get(e, "options")?;
    let u = h.undefined();
    let configured = h.call(&resolve, &u, &[opts])?;
    let sufficient = if h.type_is(&max, "number") {
        let fee = h.get(&configured, "feeTenthsBps")?;
        let ge = h.compare(RequestComparison::GreaterEqual, &max, &fee)?;
        yes_no(h, &ge)
    } else {
        lit(h, "unknown")
    };
    let approved = h.get(e, "_builderFeeApproved")?;
    let t = h.boolean(true);
    let status = if h.strict_equal(&approved, &t) {
        lit(h, "approved")
    } else {
        let approved = h.get(e, "_builderFeeApproved")?;
        let f = h.boolean(false);
        lit(
            h,
            if h.strict_equal(&approved, &f) {
                "not_approved"
            } else {
                "unknown"
            },
        )
    };
    let result = h.object()?;
    let tags = h.object()?;
    define_text(h, &tags, "hyperliquid_error_family", "builder_fee")?;
    let attached = yes_no(h, &builder);
    h.define_data(&tags, "hyperliquid_builder_attached", &attached)?;
    h.define_data(&tags, "hyperliquid_builder_check", &status)?;
    h.define_data(&tags, "hyperliquid_builder_fee_sufficient", &sufficient)?;
    for (name, key) in [
        ("hyperliquid_has_application_id", "applicationId"),
        ("hyperliquid_has_vault", "vault"),
    ] {
        let v = options(h, e, key)?;
        let v = yes_no(h, &v);
        h.define_data(&tags, name, &v)?;
    }
    let net = options(h, e, "testnet")?;
    let net = lit(h, if h.truthy(&net) { "testnet" } else { "mainnet" });
    h.define_data(&tags, "hyperliquid_network", &net)?;
    h.define_data(&result, "sentryTags", &tags)?;
    let extra = h.object()?;
    let fee = h.object()?;
    let v = string_member(h, &action, "type")?;
    h.define_data(&fee, "actionType", &v)?;
    let v = string_member(h, &action, "grouping")?;
    h.define_data(&fee, "grouping", &v)?;
    let array = h.global("Array")?;
    let is_array = h.get(&array, "isArray")?;
    let orders = optional(h, &action, "orders")?;
    let u = h.undefined();
    let yes = h.call(&is_array, &array, &[orders])?;
    let count = if h.truthy(&yes) {
        let orders = h.get(&action, "orders")?;
        h.get(&orders, "length")?
    } else {
        u
    };
    h.define_data(&fee, "orderCount", &count)?;
    let present = free(h, "Boolean", &[builder.clone()])?;
    h.define_data(&fee, "builderPayloadPresent", &present)?;
    let v = masked(h, &builder, "b", true)?;
    h.define_data(&fee, "attachedBuilderAddress", &v)?;
    let v = optional(h, &builder, "f")?;
    h.define_data(&fee, "attachedBuilderFeeTenthsBps", &v)?;
    let v = masked(h, &configured, "address", false)?;
    h.define_data(&fee, "expectedBuilderAddress", &v)?;
    let v = h.get(&configured, "feeTenthsBps")?;
    h.define_data(&fee, "requiredBuilderFeeTenthsBps", &v)?;
    for (name, key) in [
        ("checkedMainWallet", "applicationId"),
        ("signerWallet", "key"),
        ("vaultAddress", "vault"),
    ] {
        let mask = h.global("maskHyperliquidAddress")?;
        let v = options(h, e, key)?;
        let u = h.undefined();
        let v = h.call(&mask, &u, &[v])?;
        h.define_data(&fee, name, &v)?;
    }
    for (name, key) in [
        ("lastCheckApproved", "approved"),
        ("lastCheckMaxFeeRate", "maxFeeRate"),
    ] {
        let v = optional(h, &last, key)?;
        h.define_data(&fee, name, &v)?;
    }
    h.define_data(&fee, "lastCheckMaxFeeTenthsBps", &max)?;
    let v = optional(h, &last, "error")?;
    h.define_data(&fee, "lastCheckError", &v)?;
    let v = options(h, e, "publicDataOnly")?;
    h.define_data(&fee, "publicDataOnly", &v)?;
    h.define_data(&extra, "hyperliquidBuilderFee", &fee)?;
    h.define_data(&result, "sentryExtra", &extra)?;
    Ok(result)
}
pub async fn post_order_action<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    payload: &H::Value,
) -> Result<H::Value, H::Error> {
    let result = async {
        let xhr = h.get(e, "xhr")?;
        let post = h.get(&xhr, "post")?;
        let endpoint = exchange_endpoint(h)?;
        let config = headers(h)?;
        let p = h.call(&post, &xhr, &[endpoint, payload.clone(), config])?;
        let response = h.await_value(p).await?;
        h.get(&response, "data")
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(error) => {
            method(h, e, "scheduleOrderRefresh", &[])?;
            Err(error)
        }
    }
}
pub fn validate_response_and_extract_order_ids<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let result = (|| {
        let parsed = free(h, "parseHyperliquidOrderResponse", &[data.clone()])?;
        h.get(&parsed, "orderIds")
    })();
    match result {
        Ok(v) => Ok(v),
        Err(error) => {
            let class = h.global("HyperliquidUnreadableOrderResponseError")?;
            if h.instance_of(&error, &class)? {
                method(h, e, "scheduleOrderRefresh", &[])?;
            }
            Err(error)
        }
    }
}
pub async fn sign_action_or_defer<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    action: &H::Value,
    nonce: &H::Value,
) -> Result<H::Value, H::Error> {
    let secret = options(h, e, "secret")?;
    if !h.truthy(&secret) {
        return Ok(h.undefined());
    }
    let sign = h.global("signL1Action")?;
    let wallet = h.global("createWalletFromPrivateKey")?;
    let secret = options(h, e, "secret")?;
    let u = h.undefined();
    let wallet = h.call(&wallet, &u, &[secret])?;
    let vault = options(h, e, "vault")?;
    let vault = if h.truthy(&vault) { vault } else { h.null() };
    let testnet = options(h, e, "testnet")?;
    let mainnet = h.boolean(!h.truthy(&testnet));
    let p = h.call(
        &sign,
        &u,
        &[wallet, action.clone(), vault, nonce.clone(), mainnet],
    )?;
    h.await_value(p).await
}
fn action<H: OrderRequestHost>(
    h: &mut H,
    market: &H::Value,
    active: &H::Value,
    input: &H::Value,
    margin: bool,
) -> Result<H::Value, H::Error> {
    let a = h.object()?;
    define_text(h, &a, "type", "updateLeverage")?;
    let info = h.get(market, "info")?;
    let asset = h.get(&info, "hyperliquidId")?;
    h.define_data(&a, "asset", &asset)?;
    let cross = if margin {
        h.boolean(!h.truthy(input))
    } else {
        let v = if h.is_nullish(active) {
            h.undefined()
        } else {
            let leverage = h.get(active, "leverage")?;
            h.get(&leverage, "type")?
        };
        let cross = lit(h, "cross");
        h.boolean(h.strict_equal(&v, &cross))
    };
    h.define_data(&a, "isCross", &cross)?;
    let leverage = if margin {
        if h.is_nullish(active) {
            h.undefined()
        } else {
            let leverage = h.get(active, "leverage")?;
            h.get(&leverage, "value")?
        }
    } else {
        h.unary_number(input)?
    };
    h.define_data(&a, "leverage", &leverage)?;
    Ok(a)
}
fn signature_spread<H: OrderRequestHost>(
    h: &mut H,
    payload: &H::Value,
    signature: &H::Value,
) -> Result<(), H::Error> {
    let spread = h.object()?;
    if h.truthy(signature) {
        h.define_data(&spread, "signature", signature)?;
    }
    h.spread_into(payload, &spread)
}
fn vault_spread<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    payload: &H::Value,
) -> Result<(), H::Error> {
    let vault = options(h, e, "vault")?;
    let spread = h.object()?;
    if h.truthy(&vault) {
        let vault = options(h, e, "vault")?;
        h.define_data(&spread, "vaultAddress", &vault)?;
    }
    h.spread_into(payload, &spread)
}
async fn leverage_request<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    symbol: &H::Value,
    input: &H::Value,
    margin: bool,
    spot: bool,
) -> Result<(), H::Error> {
    let store = h.get(e, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, symbol)?;
    if !h.truthy(&market) {
        let constructor = h.global("Error")?;
        let mut text = "Market ".encode_utf16().collect::<Vec<_>>();
        text.extend(h.template_string(symbol)?.0);
        text.extend(" not found".encode_utf16());
        let message = h.literal(&JsString(text));
        return Err(h.construct_error(&constructor, &[message])?);
    }
    if !spot {
        let is_spot = h.get(&market, "isSpot")?;
        if h.truthy(&is_spot) {
            return Ok(());
        }
    }
    let coin = h.get(&market, "id")?;
    let active = h.get(e, "activeAssetData")?;
    let active = method(h, &active, "get", &[coin])?;
    let timestamp = clock(h)?;
    let action = action(h, &market, &active, input, margin)?;
    let p = if spot {
        let sign = h.global("signL1Action")?;
        let wallet = h.global("createWalletFromPrivateKey")?;
        let secret = options(h, e, "secret")?;
        let u = h.undefined();
        let wallet = h.call(&wallet, &u, &[secret])?;
        let null = h.null();
        h.call(
            &sign,
            &u,
            &[wallet, action.clone(), null, timestamp.clone()],
        )?
    } else {
        method(
            h,
            e,
            "signActionOrDefer",
            &[action.clone(), timestamp.clone()],
        )?
    };
    let signature = h.await_value(p).await?;
    let xhr = h.get(e, "xhr")?;
    let post = h.get(&xhr, "post")?;
    let endpoint = exchange_endpoint(h)?;
    let payload = h.object()?;
    h.define_data(&payload, "action", &action)?;
    h.define_data(&payload, "nonce", &timestamp)?;
    if spot {
        h.define_data(&payload, "signature", &signature)?;
    } else {
        signature_spread(h, &payload, &signature)?;
        vault_spread(h, e, &payload)?;
    }
    let config = headers(h)?;
    let p = h.call(&post, &xhr, &[endpoint, payload, config])?;
    h.await_value(p).await?;
    if !spot {
        method(h, e, "invalidateFakePositionsCache", &[])?;
    }
    Ok(())
}
pub async fn core_set_leverage<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    symbol: &H::Value,
    input: &H::Value,
    _side: &H::Value,
) -> Result<(), H::Error> {
    leverage_request(h, e, symbol, input, false, false).await
}
pub async fn core_change_margin_type<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    isolated: &H::Value,
    symbol: &H::Value,
) -> Result<(), H::Error> {
    leverage_request(h, e, symbol, isolated, true, false).await
}
pub async fn spot_set_leverage<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    symbol: &H::Value,
    input: &H::Value,
    _side: &H::Value,
) -> Result<(), H::Error> {
    leverage_request(h, e, symbol, input, false, true).await
}
pub async fn spot_change_margin_type<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    isolated: &H::Value,
    symbol: &H::Value,
) -> Result<(), H::Error> {
    leverage_request(h, e, symbol, isolated, true, true).await
}
pub async fn schedule_cancel<H: OrderRequestHost>(
    h: &mut H,
    e: &H::Value,
    time: &H::Value,
) -> Result<(), H::Error> {
    let action = h.object()?;
    define_text(h, &action, "type", "scheduleCancel")?;
    if !h.is_nullish(time) {
        let number = h.global("Number")?;
        let yes = method(h, &number, "isSafeInteger", &[time.clone()])?;
        if !h.truthy(&yes) {
            return Err(static_error(
                h,
                "Hyperliquid scheduleCancel time must be a safe integer UTC timestamp in milliseconds",
            )?);
        }
        let now = clock(h)?;
        let five = h.number(5000.0);
        let minimum = h.operator(BalanceOperator::Add, &now, &five)?;
        let too_soon = h.compare(RequestComparison::Less, time, &minimum)?;
        if h.truthy(&too_soon) {
            return Err(static_error(
                h,
                "Hyperliquid scheduleCancel time must be at least 5 seconds in the future",
            )?);
        }
        h.set(&action, "time", time)?;
    }
    let nonce = method(h, e, "generateNonce", &[])?;
    let p = method(h, e, "signActionOrDefer", &[action.clone(), nonce.clone()])?;
    let signature = h.await_value(p).await?;
    let payload = h.object()?;
    h.define_data(&payload, "action", &action)?;
    h.define_data(&payload, "nonce", &nonce)?;
    signature_spread(h, &payload, &signature)?;
    vault_spread(h, e, &payload)?;
    let xhr = h.get(e, "xhr")?;
    let post = h.get(&xhr, "post")?;
    let endpoint = exchange_endpoint(h)?;
    let config = headers(h)?;
    let p = h.call(&post, &xhr, &[endpoint, payload, config])?;
    h.await_value(p).await?;
    Ok(())
}
