//! Signed cancellation and optimistic cleanup in source evaluation order.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone, Debug)]
pub enum CancelCallback<V> {
    Wire { markets: V },
    RemoveId,
}
pub trait CancellationHost: SpotAccountHost {
    fn unary_plus(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn instance_of(&mut self, v: &Self::Value, class: &Self::Value) -> Result<bool, Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, v: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, v: &Self::Value);
    fn cancel_callback(
        &mut self,
        c: CancelCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
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
pub fn run_cancel_callback<H: CancellationHost>(
    h: &mut H,
    c: &CancelCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let order = args.first().cloned().unwrap_or_else(|| h.undefined());
    match c {
        CancelCallback::Wire { markets } => {
            let symbol = h.get(&order, "symbol")?;
            let market = h.index(markets, &symbol)?;
            let output = h.object()?;
            let asset = if h.is_nullish(&market) {
                h.undefined()
            } else {
                let info = h.get(&market, "info")?;
                h.get(&info, "hyperliquidId")?
            };
            let asset = h.unary_plus(&asset)?;
            h.define_data(&output, "a", &asset)?;
            let id = h.get(&order, "id")?;
            let id = h.unary_plus(&id)?;
            h.define_data(&output, "o", &id)?;
            Ok(output)
        }
        CancelCallback::RemoveId => {
            let output = h.object()?;
            let id = h.get(&order, "id")?;
            h.define_data(&output, "id", &id)?;
            Ok(output)
        }
    }
}
async fn body<H: CancellationHost>(
    h: &mut H,
    e: &H::Value,
    orders: &H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(e, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let cancels = method(h, orders, "map", |h| {
        Ok(vec![h.cancel_callback(CancelCallback::Wire { markets })?])
    })?;
    let action = h.object()?;
    h.define_data(&action, "type", &lit(h, "cancel"))?;
    h.define_data(&action, "cancels", &cancels)?;
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
        Ok(vec![
            wallet,
            action.clone(),
            vault,
            nonce.clone(),
            h.boolean(!h.truthy(&testnet)),
        ])
    })?;
    let signature = h.await_value(signature).await?;
    let payload = h.object()?;
    h.define_data(&payload, "action", &action)?;
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
    let posted = async {
        let xhr = h.get(e, "xhr")?;
        let value = method(h, &xhr, "post", |h| {
            let endpoints = h.imported("ENDPOINTS")?;
            let url = h.get(&endpoints, "EXCHANGE")?;
            let options = h.object()?;
            let headers = h.object()?;
            h.define_data(&headers, "Content-Type", &lit(h, "application/json"))?;
            h.define_data(&options, "headers", &headers)?;
            Ok(vec![url, payload, options])
        })?;
        h.await_value(value).await
    }
    .await;
    if let Err(error) = posted {
        let value = h.error_value(&error);
        let class = h.global("Error")?;
        let message = if h.instance_of(&value, &class)? {
            h.get(&value, "message")?
        } else {
            let f = h.global("String")?;
            h.call(&f, &h.undefined(), &[value])?
        };
        let terminal = import_call(h, "isOnlyHyperliquidTerminalCancelError", |_| {
            Ok(vec![message])
        })?;
        if !h.truthy(&terminal) {
            return Err(error);
        }
    }
    let iterator = h.iterator(orders)?;
    while let Some(order) = h.next(&iterator)? {
        let result = method(h, e, "suppressCancelledOrder", |h| {
            Ok(vec![h.get(&order, "id")?])
        });
        if let Err(error) = result {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    let set = h.global("Set")?;
    let cleared = h.construct(&set, &[])?;
    let iterator = h.iterator(orders)?;
    while let Some(order) = h.next(&iterator)? {
        let result = (|| {
            let store = h.get(e, "store")?;
            let markets = h.get(&store, "marketsBySymbol")?;
            let symbol = h.get(&order, "symbol")?;
            let market = h.index(&markets, &symbol)?;
            let hip = if h.is_nullish(&market) {
                h.undefined()
            } else {
                let id = h.get(&market, "id")?;
                method(h, &id, "includes", |h| Ok(vec![lit(h, ":")]))?
            };
            if h.truthy(&hip) {
                let coin = import_call(h, "symbolToCoin", |h| Ok(vec![h.get(&order, "symbol")?]))?;
                let mut key = h.template_string(&coin)?.0;
                key.push(124);
                let side = h.get(&order, "side")?;
                key.extend(h.template_string(&side)?.0);
                let key = h.literal(&JsString(key));
                let has = method(h, &cleared, "has", |_| Ok(vec![key.clone()]))?;
                if !h.truthy(&has) {
                    method(h, &cleared, "add", |_| Ok(vec![key]))?;
                    let cache = import_call(h, "getCrossCache", |_| Ok(vec![]))?;
                    method(h, &cache, "clearPrePlacement", |h| {
                        Ok(vec![coin, h.get(&order, "side")?])
                    })?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    let store = h.get(e, "store")?;
    method(h, &store, "removeOrders", |h| {
        let mapped = method(h, orders, "map", |h| {
            Ok(vec![h.cancel_callback(CancelCallback::RemoveId)?])
        })?;
        Ok(vec![mapped])
    })?;
    Ok(h.undefined())
}
pub fn cancel_orders<H: CancellationHost + HelperTaskHost<H::Value, H::Error>>(
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
    crate::helper_tasks::start(host, async move { body(&mut h, &e, &orders).await })
}
