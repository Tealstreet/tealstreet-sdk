//! Source limit-wire construction and signed limit placement.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
/// Native unary conversion, binding and iterator operations retain original errors
/// and handles. Callback-free async entry uses the shared eager task contract.
pub trait LimitOrderHost: SpotAccountHost {
    fn unary_plus(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn first_binding(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, v: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, v: &Self::Value);
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn named<H: SpotAccountHost>(h: &mut H, table: &str, key: &str) -> Result<H::Value, H::Error> {
    let table = h.imported(table)?;
    h.get(&table, key)
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
fn global_call<H: SpotAccountHost>(
    h: &mut H,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.global(key)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
fn import_call<H: SpotAccountHost>(
    h: &mut H,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(key)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
fn zero_or<H: SpotAccountHost>(h: &H, v: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { h.number(0.) }
}
fn market<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(e, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let symbol = h.get(opts, "symbol")?;
    h.index(&markets, &symbol)
}
fn ensure_limit<H: SpotAccountHost>(h: &mut H, opts: &H::Value) -> Result<(), H::Error> {
    let kind = h.get(opts, "type")?;
    let limit = named(h, "OrderType", "Limit")?;
    if !h.strict_equal(&kind, &limit) {
        let error = h.global("Error")?;
        let message = lit(h, "Not supported");
        let error = h.construct(&error, &[message])?;
        return Err(h.throw_value(error));
    }
    Ok(())
}
fn price<H: LimitOrderHost>(
    h: &mut H,
    e: &H::Value,
    m: &H::Value,
    opts: &H::Value,
    field: &str,
    fallback: bool,
) -> Result<H::Value, H::Error> {
    let value = method(h, e, "priceToPrecision", |h| {
        let symbol = h.get(m, "symbol")?;
        let value = h.get(opts, field)?;
        let value = if fallback { zero_or(h, value) } else { value };
        Ok(vec![symbol, value])
    })?;
    h.unary_plus(&value)
}
fn request<H: LimitOrderHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
    m: &H::Value,
    coin: &H::Value,
    size: &H::Value,
    trigger: Option<(&str, &str)>,
    cloid: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let request = h.object()?;
    h.define_data(&request, "coin", coin)?;
    let side = h.get(opts, "side")?;
    let buy = named(h, "OrderSide", "Buy")?;
    let is_buy = h.strict_equal(&side, &buy);
    let is_buy = h.boolean(if trigger.is_some() { !is_buy } else { is_buy });
    h.define_data(&request, "is_buy", &is_buy)?;
    h.define_data(&request, "sz", size)?;
    let limit_px = price(h, e, m, opts, "price", true)?;
    h.define_data(&request, "limit_px", &limit_px)?;
    let order_type = h.object()?;
    if let Some((field, kind)) = trigger {
        let t = h.object()?;
        h.define_data(&t, "isMarket", &h.boolean(true))?;
        h.define_data(&t, "tpsl", &lit(h, kind))?;
        let trigger_px = price(h, e, m, opts, field, false)?;
        h.define_data(&t, "triggerPx", &trigger_px)?;
        h.define_data(&order_type, "trigger", &t)?;
    } else {
        let limit = h.object()?;
        let tif = h.get(opts, "timeInForce")?;
        let post = named(h, "OrderTimeInForce", "PostOnly")?;
        let tif = lit(
            h,
            if h.strict_equal(&tif, &post) {
                "Alo"
            } else {
                "Gtc"
            },
        );
        h.define_data(&limit, "tif", &tif)?;
        h.define_data(&order_type, "limit", &limit)?;
    }
    h.define_data(&request, "order_type", &order_type)?;
    let reduce = method(h, e, "resolveReduceOnly", |h| {
        let requested = if trigger.is_some() {
            h.boolean(true)
        } else {
            global_call(h, "Boolean", |h| Ok(vec![h.get(opts, "reduceOnly")?]))?
        };
        Ok(vec![m.clone(), requested])
    })?;
    h.define_data(&request, "reduce_only", &reduce)?;
    // The entry wire carries the placement cloid; each TP/SL leg mints its own here.
    let cloid = match cloid {
        Some(cloid) => cloid.clone(),
        None => import_call(h, "mintHyperliquidCloid", |_| Ok(vec![]))?,
    };
    h.define_data(&request, "cloid", &cloid)?;
    Ok(request)
}
/// `cloid = mintHyperliquidCloid()`: the default runs only for an undefined argument.
fn default_cloid<H: LimitOrderHost>(h: &mut H, cloid: &H::Value) -> Result<H::Value, H::Error> {
    if h.is_undefined(cloid) {
        import_call(h, "mintHyperliquidCloid", |_| Ok(vec![]))
    } else {
        Ok(cloid.clone())
    }
}
pub fn create_limit_order_wires<H: LimitOrderHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
    cloid: &H::Value,
) -> Result<H::Value, H::Error> {
    let cloid = default_cloid(h, cloid)?;
    ensure_limit(h, opts)?;
    let m = market(h, e, opts)?;
    if !h.truthy(&m) {
        let error = h.global("Error")?;
        let mut message = "Market ".encode_utf16().collect::<Vec<_>>();
        let symbol = h.get(opts, "symbol")?;
        message.extend(h.template_string(&symbol)?.0);
        message.extend(" not found".encode_utf16());
        let message = h.literal(&JsString(message));
        let error = h.construct(&error, &[message])?;
        return Err(h.throw_value(error));
    }
    let index = if h.is_nullish(&m) {
        h.undefined()
    } else {
        let info = h.get(&m, "info")?;
        h.get(&info, "hyperliquidId")?
    };
    let coin = import_call(h, "symbolToCoin", |h| Ok(vec![h.get(&m, "symbol")?]))?;
    let size = import_call(h, "adjust", |h| {
        let amount = h.get(opts, "amount")?;
        let precision = h.get(&m, "precision")?;
        let precision = h.get(&precision, "amount")?;
        Ok(vec![amount, precision])
    })?;
    let wire = import_call(h, "orderRequestToOrderWire", |h| {
        let request = request(h, e, opts, &m, &coin, &size, None, Some(&cloid))?;
        Ok(vec![request, index.clone()])
    })?;
    let wires = h.array(&[wire])?;
    for (field, kind) in [("stopLoss", "sl"), ("takeProfit", "tp")] {
        let value = h.get(opts, field)?;
        if h.truthy(&value) {
            method(h, &wires, "push", |h| {
                let wire = import_call(h, "orderRequestToOrderWire", |h| {
                    let request = request(h, e, opts, &m, &coin, &size, Some((field, kind)), None)?;
                    Ok(vec![request, index.clone()])
                })?;
                Ok(vec![wire])
            })?;
        }
    }
    Ok(wires)
}
fn is_hip3<H: SpotAccountHost>(h: &mut H, m: &H::Value) -> Result<bool, H::Error> {
    if h.is_nullish(m) {
        return Ok(false);
    }
    let id = h.get(m, "id")?;
    let result = method(h, &id, "includes", |h| Ok(vec![lit(h, ":")]))?;
    Ok(h.truthy(&result))
}
fn tif<H: SpotAccountHost>(h: &mut H, opts: &H::Value) -> Result<H::Value, H::Error> {
    let tif = h.get(opts, "timeInForce")?;
    let post = named(h, "OrderTimeInForce", "PostOnly")?;
    if h.strict_equal(&tif, &post) {
        named(h, "OrderTimeInForce", "PostOnly")
    } else {
        named(h, "OrderTimeInForce", "GoodTillCancel")
    }
}
async fn place_body<H: LimitOrderHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
    cloid: &H::Value,
) -> Result<H::Value, H::Error> {
    let cloid = default_cloid(h, cloid)?;
    ensure_limit(h, opts)?;
    let wires = method(h, e, "createLimitOrderWires", |_| {
        Ok(vec![opts.clone(), cloid.clone()])
    })?;
    let builder = method(h, e, "getBuilderInfo", |_| Ok(vec![]))?;
    let builder = h.await_value(builder).await?;
    let take = h.get(opts, "takeProfit")?;
    let grouped = if h.truthy(&take) {
        true
    } else {
        let stop = h.get(opts, "stopLoss")?;
        h.truthy(&stop)
    };
    let action = import_call(
        h,
        if grouped {
            "orderWiresToNormalTPSLAction"
        } else {
            "orderWiresToOrderAction"
        },
        |_| Ok(vec![wires.clone(), builder]),
    )?;
    let nonce = method(h, e, "generateNonce", |_| Ok(vec![]))?;
    let signature = import_call(h, "signL1Action", |h| {
        let wallet = import_call(h, "createWalletFromPrivateKey", |h| {
            let options = h.get(e, "options")?;
            Ok(vec![h.get(&options, "secret")?])
        })?;
        let options = h.get(e, "options")?;
        let vault = h.get(&options, "vault")?;
        let vault = if h.truthy(&vault) { vault } else { h.null() };
        let options = h.get(e, "options")?;
        let testnet = h.get(&options, "testnet")?;
        let mainnet = h.boolean(!h.truthy(&testnet));
        Ok(vec![wallet, action.clone(), vault, nonce.clone(), mainnet])
    })?;
    let signature = h.await_value(signature).await?;
    let payload = h.object()?;
    h.define_data(&payload, "action", &action)?;
    h.define_data(&payload, "nonce", &nonce)?;
    h.define_data(&payload, "signature", &signature)?;
    let options = h.get(e, "options")?;
    let vault = h.get(&options, "vault")?;
    let has_vault = h.truthy(&vault);
    let spread = h.object()?;
    if has_vault {
        let options = h.get(e, "options")?;
        let vault = h.get(&options, "vault")?;
        h.define_data(&spread, "vaultAddress", &vault)?;
    }
    h.spread_into(&payload, &spread)?;
    // Record every wire BEFORE the REST call so the WS handler identifies each order.
    let market = market(h, e, opts)?;
    if h.truthy(&market) {
        method(h, e, "recordPlacement", |h| {
            Ok(vec![wires.clone(), h.get(&market, "symbol")?])
        })?;
    }
    if is_hip3(h, &market)? {
        let coin = import_call(h, "symbolToCoin", |h| Ok(vec![h.get(&market, "symbol")?]))?;
        method(h, e, "addToPrePlacementCache", |h| {
            let side = h.get(opts, "side")?;
            let kind = named(h, "OrderType", "Limit")?;
            let raw = h.get(opts, "price")?;
            let price = zero_or(h, raw);
            let amount = h.get(opts, "amount")?;
            Ok(vec![coin, side, kind, price, amount, cloid.clone()])
        })?;
    }
    let response = method(h, e, "postOrderAction", |_| Ok(vec![payload]))?;
    let response = h.await_value(response).await?;
    let ids = method(h, e, "validateResponseAndExtractOrderIds", |_| {
        Ok(vec![response])
    })?;
    let first = h.first_binding(&ids)?;
    if h.truthy(&first) {
        method(h, e, "rememberLimitOrderTimeInForce", |h| {
            let tif = tif(h, opts)?;
            Ok(vec![first, tif])
        })?;
    }
    if is_hip3(h, &market)? {
        let iterator = h.iterator(&ids)?;
        while let Some(id) = h.next(&iterator)? {
            let result = (|| {
                method(h, e, "addPendingHip3Order", |h| {
                    let kind = named(h, "OrderType", "Limit")?;
                    let raw = h.get(opts, "price")?;
                    let price = zero_or(h, raw);
                    Ok(vec![id.clone(), kind, price])
                })?;
                let order = h.object()?;
                h.define_data(&order, "id", &id)?;
                let status = named(h, "OrderStatus", "Creating")?;
                h.define_data(&order, "status", &status)?;
                let symbol = h.get(&market, "symbol")?;
                h.define_data(&order, "symbol", &symbol)?;
                let kind = named(h, "OrderType", "Limit")?;
                h.define_data(&order, "type", &kind)?;
                let side = h.get(opts, "side")?;
                h.define_data(&order, "side", &side)?;
                let raw = h.get(opts, "price")?;
                let price = zero_or(h, raw);
                h.define_data(&order, "price", &price)?;
                let amount = h.get(opts, "amount")?;
                h.define_data(&order, "amount", &amount)?;
                h.define_data(&order, "filled", &h.number(0.))?;
                let amount = h.get(opts, "amount")?;
                h.define_data(&order, "remaining", &amount)?;
                let reduce = global_call(h, "Boolean", |h| Ok(vec![h.get(opts, "reduceOnly")?]))?;
                h.define_data(&order, "reduceOnly", &reduce)?;
                let tif = tif(h, opts)?;
                h.define_data(&order, "timeInForce", &tif)?;
                let clock = h.imported("virtualClock")?;
                let now = method(h, &clock, "getCurrentTimeMs", |_| Ok(vec![]))?;
                h.define_data(&order, "lastUpdate", &now)?;
                let store = h.get(e, "store")?;
                method(h, &store, "addOrUpdateOrder", |_| Ok(vec![order]))?;
                Ok(())
            })();
            if let Err(error) = result {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
        method(h, e, "refreshHip3OrdersWithRetry", |_| {
            Ok(vec![ids.clone()])
        })?;
    }
    Ok(ids)
}
pub fn place_limit_order<H: LimitOrderHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
    opts: &H::Value,
    cloid: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    let opts = opts.clone();
    let cloid = cloid.clone();
    crate::helper_tasks::start(host, async move { place_body(&mut h, &e, &opts, &cloid).await })
}
