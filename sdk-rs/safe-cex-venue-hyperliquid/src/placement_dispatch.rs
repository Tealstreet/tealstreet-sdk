//! Single-order dispatch and serial placement source bodies.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone, Debug)]
pub struct SerialOrderCallback<V> {
    pub exchange: V,
}
pub trait PlacementHost: SpotAccountHost {
    fn unary_plus(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, v: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, v: &Self::Value);
    fn serial_order_callback(
        &mut self,
        c: SerialOrderCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn named<H: SpotAccountHost>(h: &mut H, t: &str, k: &str) -> Result<H::Value, H::Error> {
    let t = h.imported(t)?;
    h.get(&t, k)
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let args = args(h)?;
    h.call(&f, o, &args)
}
fn import_call<H: SpotAccountHost>(
    h: &mut H,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(k)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
fn global_call<H: SpotAccountHost>(
    h: &mut H,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.global(k)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
fn zero_or<H: SpotAccountHost>(h: &H, v: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { h.number(0.) }
}
fn kind_is<H: SpotAccountHost>(h: &mut H, opts: &H::Value, k: &str) -> Result<bool, H::Error> {
    let v = h.get(opts, "type")?;
    let k = named(h, "OrderType", k)?;
    Ok(h.strict_equal(&v, &k))
}
fn failure<H: SpotAccountHost>(
    h: &mut H,
    message: impl FnOnce(&mut H) -> Result<H::Value, H::Error>,
) -> Result<H::Error, H::Error> {
    let f = h.global("Error")?;
    let text = message(h)?;
    let v = h.construct(&f, &[text])?;
    Ok(h.throw_value(v))
}
fn append<H: SpotAccountHost>(h: &H, units: &mut Vec<u16>, s: &str) {
    let _ = h;
    units.extend(s.encode_utf16())
}
fn market<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let s = h.get(e, "store")?;
    let m = h.get(&s, "marketsBySymbol")?;
    let symbol = h.get(opts, "symbol")?;
    h.index(&m, &symbol)
}
fn hip3<H: SpotAccountHost>(h: &mut H, m: &H::Value) -> Result<bool, H::Error> {
    let id = h.get(m, "id")?;
    let found = method(h, &id, "includes", |h| Ok(vec![lit(h, ":")]))?;
    Ok(h.truthy(&found))
}
fn asset<H: SpotAccountHost>(h: &mut H, m: &H::Value) -> Result<H::Value, H::Error> {
    if h.is_nullish(m) {
        return Ok(h.undefined());
    }
    let info = h.get(m, "info")?;
    h.get(&info, "hyperliquidId")
}
fn base_limit<H: PlacementHost>(
    h: &mut H,
    e: &H::Value,
    m: &H::Value,
    opts: &H::Value,
    base: &H::Value,
) -> Result<H::Value, H::Error> {
    let value = method(h, e, "priceToPrecision", |h| {
        let symbol = h.get(m, "symbol")?;
        let side = h.get(opts, "side")?;
        let buy = named(h, "OrderSide", "Buy")?;
        let factor = h.number(if h.strict_equal(&side, &buy) {
            1.05
        } else {
            0.95
        });
        let value = h.binary("*", base, &factor)?;
        Ok(vec![symbol, value])
    })?;
    h.unary_plus(&value)
}
fn wire<H: PlacementHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
    m: &H::Value,
    index: &H::Value,
    base: &H::Value,
    market_amount: Option<&H::Value>,
    trigger: Option<(&H::Value, &H::Value)>,
) -> Result<H::Value, H::Error> {
    import_call(h, "orderRequestToOrderWire", |h| {
        let request = h.object()?;
        let coin = import_call(h, "symbolToCoin", |h| Ok(vec![h.get(m, "symbol")?]))?;
        h.define_data(&request, "coin", &coin)?;
        let side = h.get(opts, "side")?;
        let buy = named(h, "OrderSide", "Buy")?;
        h.define_data(&request, "is_buy", &h.boolean(h.strict_equal(&side, &buy)))?;
        let size = if let Some(n) = market_amount {
            import_call(h, "adjust", |h| Ok(vec![h.get(opts, "amount")?, n.clone()]))?
        } else {
            trigger.expect("trigger pair").0.clone()
        };
        h.define_data(&request, "sz", &size)?;
        let limit = base_limit(h, e, m, opts, base)?;
        h.define_data(&request, "limit_px", &limit)?;
        let order_type = h.object()?;
        if let Some((_, price)) = trigger {
            let t = h.object()?;
            h.define_data(&t, "isMarket", &h.boolean(true))?;
            let tp = kind_is(h, opts, "TakeProfit")?;
            h.define_data(&t, "tpsl", &lit(h, if tp { "tp" } else { "sl" }))?;
            let value = method(h, e, "priceToPrecision", |h| {
                Ok(vec![h.get(m, "symbol")?, price.clone()])
            })?;
            let value = h.unary_plus(&value)?;
            h.define_data(&t, "triggerPx", &value)?;
            h.define_data(&order_type, "trigger", &t)?;
        } else {
            let limit = h.object()?;
            h.define_data(&limit, "tif", &lit(h, "Gtc"))?;
            h.define_data(&order_type, "limit", &limit)?;
        }
        h.define_data(&request, "order_type", &order_type)?;
        let reduce = method(h, e, "resolveReduceOnly", |h| {
            let requested = global_call(h, "Boolean", |h| Ok(vec![h.get(opts, "reduceOnly")?]))?;
            Ok(vec![m.clone(), requested])
        })?;
        h.define_data(&request, "reduce_only", &reduce)?;
        Ok(vec![request, index.clone()])
    })
}
async fn sign_payload<H: PlacementHost>(
    h: &mut H,
    e: &H::Value,
    action: &H::Value,
) -> Result<H::Value, H::Error> {
    let nonce = method(h, e, "generateNonce", |_| Ok(vec![]))?;
    let sig = import_call(h, "signL1Action", |h| {
        let wallet = import_call(h, "createWalletFromPrivateKey", |h| {
            let options = h.get(e, "options")?;
            Ok(vec![h.get(&options, "secret")?])
        })?;
        let options = h.get(e, "options")?;
        let vault = h.get(&options, "vault")?;
        let vault = if h.truthy(&vault) { vault } else { h.null() };
        let options = h.get(e, "options")?;
        let testnet = h.get(&options, "testnet")?;
        Ok(vec![
            wallet,
            action.clone(),
            vault,
            nonce.clone(),
            h.boolean(!h.truthy(&testnet)),
        ])
    })?;
    let signature = h.await_value(sig).await?;
    let payload = h.object()?;
    h.define_data(&payload, "action", action)?;
    h.define_data(&payload, "nonce", &nonce)?;
    h.define_data(&payload, "signature", &signature)?;
    let options = h.get(e, "options")?;
    let vault = h.get(&options, "vault")?;
    let spread = h.object()?;
    if h.truthy(&vault) {
        let options = h.get(e, "options")?;
        let vault = h.get(&options, "vault")?;
        h.define_data(&spread, "vaultAddress", &vault)?;
    }
    h.spread_into(&payload, &spread)?;
    Ok(payload)
}
async fn post<H: PlacementHost>(
    h: &mut H,
    e: &H::Value,
    payload: H::Value,
) -> Result<(H::Value, H::Value), H::Error> {
    let response = method(h, e, "postOrderAction", |_| Ok(vec![payload]))?;
    let response = h.await_value(response).await?;
    let ids = method(h, e, "validateResponseAndExtractOrderIds", |_| {
        Ok(vec![response.clone()])
    })?;
    Ok((response, ids))
}
async fn body<H: PlacementHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    method(h, e, "resolveCallerClientOrderId", |_| {
        Ok(vec![opts.clone()])
    })?;
    if kind_is(h, opts, "TrailingStopLoss")? {
        return Err(failure(h, |h| Ok(lit(h, "Not supported")))?);
    }
    let m = market(h, e, opts)?;
    if !h.truthy(&m) {
        return Err(failure(h, |h| {
            let mut text = "Market ".encode_utf16().collect::<Vec<_>>();
            let symbol = h.get(opts, "symbol")?;
            text.extend(h.template_string(&symbol)?.0);
            append(h, &mut text, " not found");
            Ok(h.literal(&JsString(text)))
        })?);
    }
    if kind_is(h, opts, "Market")? {
        let index = asset(h, &m)?;
        let precision = h.get(&m, "precision")?;
        let amount = h.get(&precision, "amount")?;
        let base = method(h, e, "getOrderBasePrice", |h| {
            Ok(vec![m.clone(), lit(h, "market order")])
        })?;
        let wire = wire(h, e, opts, &m, &index, &base, Some(&amount), None)?;
        let builder = method(h, e, "getBuilderInfo", |_| Ok(vec![]))?;
        let builder = h.await_value(builder).await?;
        let action = import_call(h, "orderWiresToOrderAction", |h| {
            Ok(vec![h.array(&[wire])?, builder])
        })?;
        let payload = sign_payload(h, e, &action).await?;
        let (_, ids) = post(h, e, payload).await?;
        let first = h.get(&ids, "0")?;
        if h.truthy(&first) {
            method(h, e, "rememberLimitOrderTimeInForce", |h| {
                Ok(vec![
                    h.get(&ids, "0")?,
                    named(h, "OrderTimeInForce", "GoodTillCancel")?,
                ])
            })?;
        }
        return Ok(ids);
    } else if kind_is(h, opts, "Limit")? {
        let value = method(h, e, "placeLimitOrder", |_| Ok(vec![opts.clone()]))?;
        return h.await_value(value).await;
    } else if kind_is(h, opts, "TakeProfit")? || kind_is(h, opts, "StopLoss")? {
        let index = asset(h, &m)?;
        let base = method(h, e, "getOrderBasePrice", |h| {
            let kind = h.get(opts, "type")?;
            let mut text = h.template_string(&kind)?.0;
            append(h, &mut text, " order");
            Ok(vec![m.clone(), h.literal(&JsString(text))])
        })?;
        let amount = h.get(opts, "amount")?;
        let size = if h.strict_equal(&amount, &h.number(0.)) {
            h.number(0.)
        } else {
            import_call(h, "adjust", |h| {
                let amount = h.get(opts, "amount")?;
                let precision = h.get(&m, "precision")?;
                Ok(vec![amount, h.get(&precision, "amount")?])
            })?
        };
        let position = method(h, e, "shouldUsePositionTpsl", |_| {
            Ok(vec![opts.clone(), m.clone(), size.clone()])
        })?;
        let wire_size = if h.truthy(&position) {
            h.number(0.)
        } else {
            size
        };
        let price = global_call(h, "Number", |h| Ok(vec![h.get(opts, "price")?]))?;
        let valid = import_call(h, "isFinitePositiveNumber", |_| Ok(vec![price.clone()]))?;
        if !h.truthy(&valid) {
            return Err(failure(h, |h| {
                let mut text = "Cannot place Hyperliquid "
                    .encode_utf16()
                    .collect::<Vec<_>>();
                let kind = h.get(opts, "type")?;
                text.extend(h.template_string(&kind)?.0);
                append(h, &mut text, " for ");
                let symbol = h.get(&m, "symbol")?;
                text.extend(h.template_string(&symbol)?.0);
                append(h, &mut text, ": invalid trigger price");
                Ok(h.literal(&JsString(text)))
            })?);
        }
        let wire = wire(
            h,
            e,
            opts,
            &m,
            &index,
            &base,
            None,
            Some((&wire_size, &price)),
        )?;
        let builder = method(h, e, "getBuilderInfo", |_| Ok(vec![]))?;
        let builder = h.await_value(builder).await?;
        let action = import_call(
            h,
            if h.truthy(&position) {
                "orderWiresToPositionalTPSLAction"
            } else {
                "orderWiresToOrderAction"
            },
            |h| Ok(vec![h.array(&[wire])?, builder]),
        )?;
        let payload = sign_payload(h, e, &action).await?;
        if hip3(h, &m)? {
            let coin = import_call(h, "symbolToCoin", |h| Ok(vec![h.get(&m, "symbol")?]))?;
            method(h, e, "addToPrePlacementCache", |h| {
                let side = h.get(opts, "side")?;
                let kind = h.get(opts, "type")?;
                let raw = h.get(opts, "price")?;
                let price = zero_or(h, raw);
                let amount = h.get(opts, "amount")?;
                Ok(vec![coin, side, kind, price, amount])
            })?;
        }
        let (response, ids) = post(h, e, payload).await?;
        let len = h.get(&ids, "length")?;
        if h.strict_equal(&len, &h.number(0.)) {
            let deferred = method(h, e, "responseHasDeferredOrderStatus", |_| {
                Ok(vec![response])
            })?;
            if h.truthy(&deferred) {
                method(h, e, "scheduleOrderRefresh", |_| Ok(vec![]))?;
            }
        }
        if hip3(h, &m)? {
            let iterator = h.iterator(&ids)?;
            while let Some(id) = h.next(&iterator)? {
                let result = (|| {
                    method(h, e, "addPendingHip3Order", |h| {
                        let kind = h.get(opts, "type")?;
                        let raw = h.get(opts, "price")?;
                        let price = zero_or(h, raw);
                        Ok(vec![id.clone(), kind, price])
                    })?;
                    let order = h.object()?;
                    h.define_data(&order, "id", &id)?;
                    let status = named(h, "OrderStatus", "Creating")?;
                    h.define_data(&order, "status", &status)?;
                    let symbol = h.get(&m, "symbol")?;
                    h.define_data(&order, "symbol", &symbol)?;
                    for field in ["type", "side"] {
                        let value = h.get(opts, field)?;
                        h.define_data(&order, field, &value)?;
                    }
                    let raw = h.get(opts, "price")?;
                    let price = zero_or(h, raw);
                    h.define_data(&order, "price", &price)?;
                    let amount = h.get(opts, "amount")?;
                    h.define_data(&order, "amount", &amount)?;
                    h.define_data(&order, "filled", &h.number(0.))?;
                    let amount = h.get(opts, "amount")?;
                    h.define_data(&order, "remaining", &amount)?;
                    let reduce =
                        global_call(h, "Boolean", |h| Ok(vec![h.get(opts, "reduceOnly")?]))?;
                    h.define_data(&order, "reduceOnly", &reduce)?;
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
        return Ok(ids);
    }
    Err(failure(h, |h| {
        let mut text = "Unknown order type ".encode_utf16().collect::<Vec<_>>();
        let kind = h.get(opts, "type")?;
        text.extend(h.template_string(&kind)?.0);
        Ok(h.literal(&JsString(text)))
    })?)
}
pub fn place_order<H: PlacementHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move { body(&mut h, &e, &opts).await })
}
pub fn run_serial_order_callback<H: PlacementHost>(
    h: &mut H,
    c: &SerialOrderCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    method(h, &c.exchange, "placeOrder", |_| Ok(vec![first]))
}
pub fn place_orders<H: PlacementHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
    orders: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    let orders = orders.clone();
    crate::helper_tasks::start(host, async move {
        method(&mut h, &e, "assertCallerClientOrderIds", |_| {
            Ok(vec![orders.clone()])
        })?;
        let ids = import_call(&mut h, "mapSeries", |h| {
            Ok(vec![
                orders,
                h.serial_order_callback(SerialOrderCallback { exchange: e })?,
            ])
        })?;
        let ids = h.await_value(ids).await?;
        method(&mut h, &ids, "flat", |_| Ok(vec![]))
    })
}
