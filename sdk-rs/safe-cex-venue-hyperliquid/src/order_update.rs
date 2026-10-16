//! Source cancel-and-replace order update and its lexical numeric callbacks.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone, Debug)]
pub enum UpdateCallback<V> {
    FinitePositive,
    MaxDivergence { reference: V },
}
pub trait OrderUpdateHost: SpotAccountHost {
    /// Native async parameter record binding, including nullish diagnostics and
    /// ordered Get operations before body evaluation.
    fn update_parameter_binding(
        &mut self,
        value: &Self::Value,
    ) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn has_property(&mut self, o: &Self::Value, k: &str) -> Result<bool, Self::Error>;
    fn first_rest_binding(
        &mut self,
        v: &Self::Value,
    ) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn update_callback(
        &mut self,
        c: UpdateCallback<Self::Value>,
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
fn global_call<H: SpotAccountHost>(
    h: &mut H,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.global(k)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
fn kind_is<H: SpotAccountHost>(h: &mut H, o: &H::Value, k: &str) -> Result<bool, H::Error> {
    let v = h.get(o, "type")?;
    let kind = named(h, "OrderType", k)?;
    Ok(h.strict_equal(&v, &kind))
}
fn numeric_finite<H: SpotAccountHost>(h: &mut H, v: &H::Value) -> Result<H::Value, H::Error> {
    let number = h.global("Number")?;
    method(h, &number, "isFinite", |_| Ok(vec![v.clone()]))
}
fn greater<H: SpotAccountHost>(h: &mut H, a: &H::Value, b: &H::Value) -> Result<bool, H::Error> {
    let v = h.binary(">", a, b)?;
    Ok(h.truthy(&v))
}
fn greater_number<H: SpotAccountHost>(h: &mut H, a: &H::Value, n: f64) -> Result<bool, H::Error> {
    let b = h.number(n);
    greater(h, a, &b)
}
pub fn run_update_callback<H: OrderUpdateHost>(
    h: &mut H,
    c: &UpdateCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match c {
        UpdateCallback::FinitePositive => {
            let finite = numeric_finite(h, &first)?;
            if !h.truthy(&finite) {
                return Ok(finite);
            }
            h.binary(">", &first, &h.number(0.))
        }
        UpdateCallback::MaxDivergence { reference } => {
            let price = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let math = h.global("Math")?;
            let absolute = method(h, &math, "abs", |h| {
                let diff = h.binary("-", &price, reference)?;
                Ok(vec![diff])
            })?;
            let divergence = h.binary("/", &absolute, reference)?;
            let math = h.global("Math")?;
            method(h, &math, "max", |_| Ok(vec![first, divergence]))
        }
    }
}
fn append(text: &mut Vec<u16>, s: &str) {
    text.extend(s.encode_utf16())
}
fn append_value<H: SpotAccountHost>(
    h: &mut H,
    text: &mut Vec<u16>,
    v: &H::Value,
) -> Result<(), H::Error> {
    text.extend(h.template_string(v)?.0);
    Ok(())
}
fn copy_field<H: SpotAccountHost>(
    h: &mut H,
    out: &H::Value,
    input: &H::Value,
    k: &str,
) -> Result<(), H::Error> {
    let value = h.get(input, k)?;
    h.define_data(out, k, &value)
}
async fn body<H: OrderUpdateHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let (order, update) = h.update_parameter_binding(opts)?;
    let new = h.object()?;
    for field in ["symbol", "type", "side", "price", "amount"] {
        copy_field(h, &new, &order, field)?;
    }
    let reduce = h.get(&order, "reduceOnly")?;
    let reduce = if h.truthy(&reduce) {
        reduce
    } else {
        h.boolean(false)
    };
    h.define_data(&new, "reduceOnly", &reduce)?;
    copy_field(h, &new, &order, "timeInForce")?;
    for field in ["tpTriggerType", "slTriggerType"] {
        if h.has_property(&order, field)? {
            let value = h.get(&order, field)?;
            h.set(&new, field, &value)?;
        }
    }
    for field in ["price", "amount"] {
        if h.has_property(&update, field)? {
            let value = h.get(&update, field)?;
            h.set(&new, field, &value)?;
        }
    }
    if kind_is(h, &order, "TakeProfit")? || kind_is(h, &order, "StopLoss")? {
        let store = h.get(e, "store")?;
        let tickers = h.get(&store, "tickersBySymbol")?;
        let symbol = h.get(&order, "symbol")?;
        let ticker = h.index(&tickers, &symbol)?;
        let mut prices = vec![];
        for field in ["mark", "last", "index"] {
            let price = global_call(h, "Number", |h| {
                let value = if h.is_nullish(&ticker) {
                    h.undefined()
                } else {
                    h.get(&ticker, field)?
                };
                Ok(vec![value])
            })?;
            prices.push(price);
        }
        let trigger = global_call(h, "Number", |h| Ok(vec![h.get(&new, "price")?]))?;
        let array = h.array(&prices)?;
        let candidates = method(h, &array, "filter", |h| {
            Ok(vec![h.update_callback(UpdateCallback::FinitePositive)?])
        })?;
        let len = h.get(&candidates, "length")?;
        if greater_number(h, &len, 0.)? {
            let finite = numeric_finite(h, &trigger)?;
            if h.truthy(&finite) && greater_number(h, &trigger, 0.)? {
                let (reference, secondary) = h.first_rest_binding(&candidates)?;
                let max = method(h, &secondary, "reduce", |h| {
                    Ok(vec![
                        h.update_callback(UpdateCallback::MaxDivergence {
                            reference: reference.clone(),
                        })?,
                        h.number(0.),
                    ])
                })?;
                if greater_number(h, &max, 0.25)? {
                    method(h, e, "log", |h| {
                        let mut text = "Skipping TP/SL type re-evaluation for "
                            .encode_utf16()
                            .collect::<Vec<_>>();
                        let symbol = h.get(&order, "symbol")?;
                        append_value(h, &mut text, &symbol)?;
                        append(&mut text, ": inconsistent reference prices (mark=");
                        append_value(h, &mut text, &prices[0])?;
                        append(&mut text, ", last=");
                        append_value(h, &mut text, &prices[1])?;
                        append(&mut text, ", index=");
                        append_value(h, &mut text, &prices[2])?;
                        append(&mut text, ")");
                        Ok(vec![
                            h.literal(&JsString(text)),
                            named(h, "LogSeverity", "Warning")?,
                        ])
                    })?;
                } else {
                    let side = h.get(&order, "side")?;
                    let sell = named(h, "OrderSide", "Sell")?;
                    let sell = h.strict_equal(&side, &sell);
                    let above = greater(h, &trigger, &reference)?;
                    let tp = if sell { above } else { !above };
                    let projected =
                        named(h, "OrderType", if tp { "TakeProfit" } else { "StopLoss" })?;
                    if kind_is(h, &order, "TakeProfit")? {
                        let stop = named(h, "OrderType", "StopLoss")?;
                        if h.strict_equal(&projected, &stop) {
                            let stop = named(h, "OrderType", "StopLoss")?;
                            h.set(&new, "type", &stop)?;
                            method(h, e, "log", |h| {
                                let mut text = "Order type changed from "
                                    .encode_utf16()
                                    .collect::<Vec<_>>();
                                let kind = h.get(&order, "type")?;
                                append_value(h, &mut text, &kind)?;
                                append(&mut text, " to ");
                                let kind = h.get(&new, "type")?;
                                append_value(h, &mut text, &kind)?;
                                append(&mut text, " based on new price ");
                                append_value(h, &mut text, &trigger)?;
                                append(&mut text, " vs market ");
                                append_value(h, &mut text, &reference)?;
                                Ok(vec![
                                    h.literal(&JsString(text)),
                                    named(h, "LogSeverity", "Info")?,
                                ])
                            })?;
                        } else if kind_is(h, &order, "StopLoss")? {
                            let profit = named(h, "OrderType", "TakeProfit")?;
                            if h.strict_equal(&projected, &profit) {
                                warn_sl(h, e, &order, &trigger)?;
                            }
                        }
                    } else if kind_is(h, &order, "StopLoss")? {
                        let profit = named(h, "OrderType", "TakeProfit")?;
                        if h.strict_equal(&projected, &profit) {
                            warn_sl(h, e, &order, &trigger)?;
                        }
                    }
                }
            }
        }
    }
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
        let placeholder = h.object()?;
        copy_field(h, &placeholder, &order, "id")?;
        let status = named(h, "OrderStatus", "Updating")?;
        h.define_data(&placeholder, "status", &status)?;
        copy_field(h, &placeholder, &order, "symbol")?;
        copy_field(h, &placeholder, &new, "type")?;
        copy_field(h, &placeholder, &order, "side")?;
        let price = h.get(&new, "price")?;
        let price = if h.truthy(&price) {
            price
        } else {
            h.get(&order, "price")?
        };
        h.define_data(&placeholder, "price", &price)?;
        let amount = h.get(&new, "amount")?;
        let amount = if h.truthy(&amount) {
            amount
        } else {
            h.get(&order, "amount")?
        };
        h.define_data(&placeholder, "amount", &amount)?;
        h.define_data(&placeholder, "filled", &h.number(0.))?;
        let amount = h.get(&new, "amount")?;
        let amount = if h.truthy(&amount) {
            amount
        } else {
            h.get(&order, "amount")?
        };
        h.define_data(&placeholder, "remaining", &amount)?;
        let reduce = global_call(h, "Boolean", |h| Ok(vec![h.get(&new, "reduceOnly")?]))?;
        h.define_data(&placeholder, "reduceOnly", &reduce)?;
        let clock = h.imported("virtualClock")?;
        let time = method(h, &clock, "getCurrentTimeMs", |_| Ok(vec![]))?;
        h.define_data(&placeholder, "lastUpdate", &time)?;
        let store = h.get(e, "store")?;
        method(h, &store, "addOrUpdateOrder", |_| Ok(vec![placeholder]))?;
    }
    let cancelled = method(h, e, "cancelOrders", |h| {
        Ok(vec![h.array(&[order.clone()])?])
    })?;
    h.await_value(cancelled).await?;
    let dead = h.get(e, "_deadOrders")?;
    method(h, &dead, "add", |h| Ok(vec![h.get(&order, "id")?]))?;
    // A fresh cloid for the replacement (a caller's ID stays on the cancelled
    // order); the result carries it because a positional TP/SL answers no oid.
    let mint = SpotAccountHost::imported(h, "mintHyperliquidCloid")?;
    let undefined = SpotAccountHost::undefined(h);
    let cloid = h.call(&mint, &undefined, &[])?;
    // The callee binding is read before its awaited argument is evaluated.
    let wrap = SpotAccountHost::imported(h, "withReplacementClientOrderId")?;
    let placed = method(h, e, "placeOrderWithCloid", |_| Ok(vec![new, cloid.clone()]))?;
    let placed = h.await_value(placed).await?;
    let undefined = SpotAccountHost::undefined(h);
    h.call(&wrap, &undefined, &[placed, cloid])
}
fn warn_sl<H: OrderUpdateHost>(
    h: &mut H,
    e: &H::Value,
    order: &H::Value,
    trigger: &H::Value,
) -> Result<(), H::Error> {
    method(h, e, "log", |h| {
        let mut text = "Skipping SL->TP auto-switch for "
            .encode_utf16()
            .collect::<Vec<_>>();
        let symbol = h.get(order, "symbol")?;
        append_value(h, &mut text, &symbol)?;
        append(&mut text, " at trigger ");
        append_value(h, &mut text, trigger)?;
        append(&mut text, "; keeping stop-loss type");
        Ok(vec![
            h.literal(&JsString(text)),
            named(h, "LogSeverity", "Warning")?,
        ])
    })?;
    Ok(())
}
pub fn update_order<H: OrderUpdateHost + HelperTaskHost<H::Value, H::Error>>(
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
