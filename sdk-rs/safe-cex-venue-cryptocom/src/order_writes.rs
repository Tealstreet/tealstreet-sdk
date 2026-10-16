//! Complete order-write/batch bodies and public consumer delegates.
//! Native dependency bindings and exchange class wiring remain separate.
use std::{future::Future, pin::Pin, rc::Rc};
pub type BoxFuture<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub trait WriteHost: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    type Iterator;
    type PostFuture: Future<Output = Result<Self::Value, Self::Error>>;
    type CallFuture: Future<Output = Result<Self::Value, Self::Error>> + 'static;
    type SeriesFuture: Future<Output = Result<Self::Value, Self::Error>>;
    fn root(&self) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn string(&self, text: &'static str) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn get(&self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    /// Includes native computed-key coercion before the property read.
    fn get_key(&self, value: Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn assign(
        &self,
        object: &Self::Value,
        key: &'static str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn has(&self, object: Self::Value, key: &'static str) -> Result<bool, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn equal_string(&self, value: &Self::Value, text: &'static str) -> bool;
    fn prepare(
        &self,
        receiver: &Self::Value,
        key: &'static str,
    ) -> Result<Self::Prepared, Self::Error>;
    fn call(
        &self,
        prepared: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    /// Resolve/call immediately; the host owns eager native promises independently
    /// of the returned future, which the caller drives at await boundaries.
    fn call_async(
        &self,
        prepared: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Self::CallFuture, Self::Error>;
    fn endpoint(&self, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn post(
        &self,
        prepared: Self::Prepared,
        endpoint: Self::Value,
        params: Self::Value,
    ) -> Result<Self::PostFuture, Self::Error>;
    fn is_finite(&self, value: Self::Value) -> Result<bool, Self::Error>;
    fn less_equal_zero(&self, value: Self::Value) -> Result<bool, Self::Error>;
    fn prepare_error(&self) -> Result<Self::Prepared, Self::Error>;
    fn template(
        &self,
        prefix: &'static str,
        value: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn construct_error(
        &self,
        prepared: Self::Prepared,
        message: Self::Value,
    ) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn severity_error(&self) -> Result<Self::Value, Self::Error>;
    fn spread(&self, iterable: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    /// Iterator remains native and live; no guard spans a callback or await.
    fn iterator(&self, iterable: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    /// IteratorClose on a body throw retains the original error even if close throws.
    fn close_throw(&self, iterator: Self::Iterator, error: Self::Error) -> Self::Error;
    /// Original imported forEachSeries dependency; it may eagerly invoke callbacks.
    fn series<F>(
        &self,
        orders: Self::Value,
        callback: F,
    ) -> Result<Self::SeriesFuture, Self::Error>
    where
        F: Fn(Self::Value) -> BoxFuture<Self::Value, Self::Error> + 'static;
}
enum Begun<F, V, E> {
    Waiting(F),
    Done(Result<V, E>),
}
fn log<H: WriteHost>(h: &H, prefix: &'static str, error: &H::Error) -> Result<(), H::Error> {
    let method = h.prepare(&h.root(), "log")?;
    let message = h.template(prefix, h.error_value(error))?;
    let severity = h.severity_error()?;
    h.call(method, vec![message, severity])?;
    Ok(())
}
fn rethrow<H: WriteHost>(
    h: &H,
    prefix: &'static str,
    error: H::Error,
) -> Result<H::Value, H::Error> {
    log(h, prefix, &error)?;
    Err(error)
}
fn caught<H: WriteHost>(
    h: &H,
    result: Result<H::PostFuture, H::Error>,
    prefix: &'static str,
) -> Begun<H::PostFuture, H::Value, H::Error> {
    match result {
        Ok(future) => Begun::Waiting(future),
        Err(error) => Begun::Done(rethrow(h, prefix, error)),
    }
}
async fn finish<H: WriteHost>(
    h: Rc<H>,
    begun: Begun<H::PostFuture, H::Value, H::Error>,
    prefix: &'static str,
    ids: bool,
) -> Result<H::Value, H::Error> {
    let future = match begun {
        Begun::Waiting(f) => f,
        Begun::Done(result) => return result,
    };
    let result = match future.await {
        Err(e) => Err(e),
        Ok(response) => {
            if ids {
                (|| {
                    let data = h.get(&response, "data")?;
                    let id = h.get(&data, "order_id")?;
                    Ok(h.array(vec![id]))
                })()
            } else {
                Ok(h.undefined())
            }
        }
    };
    match result {
        Ok(v) => Ok(v),
        Err(e) => rethrow(&*h, prefix, e),
    }
}
fn to_string<H: WriteHost>(h: &H, value: H::Value) -> Result<H::Value, H::Error> {
    let method = h.prepare(&value, "toString")?;
    h.call(method, vec![])
}
fn construct<H: WriteHost, F: FnOnce() -> Result<H::Value, H::Error>>(
    h: &H,
    prefix: &'static str,
    operand: F,
) -> Result<H::Error, H::Error> {
    let ctor = h.prepare_error()?;
    let value = operand()?;
    let message = h.template(prefix, value)?;
    h.construct_error(ctor, message)
}
fn trigger<H: WriteHost>(h: &H, opts: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal_string(&h.get(opts, "type")?, "stop_market")
        || h.equal_string(&h.get(opts, "type")?, "take_profit_market"))
}
pub fn place_order<H: WriteHost>(
    h: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    // Caller-ID resolution is deliberately outside the source try/catch.
    let caller = (|| {
        let method = h.prepare(&h.root(), "resolveCallerClientOrderId")?;
        h.call(method, vec![opts.clone()])
    })();
    let begun = match caller {
        Err(e) => Begun::Done(Err(e)),
        Ok(caller) => {
            let started = (|| {
                let store = h.get(&h.root(), "store")?;
                let by_symbol = h.get(&store, "marketsBySymbol")?;
                let symbol = h.get(&opts, "symbol")?;
                let market = h.get_key(by_symbol, symbol)?;
                if !h.truthy(&market) {
                    return Err(construct(&*h, "Market not found: ", || {
                        h.get(&opts, "symbol")
                    })?);
                }
                let instrument = h.get(&market, "id")?;
                let side = if h.equal_string(&h.get(&opts, "side")?, "buy") {
                    h.string("BUY")
                } else {
                    h.string("SELL")
                };
                let map_type = h.prepare(&h.root(), "mapOrderType")?;
                let kind = h.call(map_type, vec![h.get(&opts, "type")?])?;
                let mut fields = vec![
                    ("instrument_name", instrument),
                    ("side", side),
                    ("type", kind),
                ];
                if !h.is_undefined(&caller) {
                    fields.push(("client_oid", caller));
                }
                let request = h.object(fields);
                let is_trigger = trigger(&*h, &opts)?;
                let quantity = to_string(&*h, h.get(&opts, "amount")?)?;
                h.assign(&request, "quantity", quantity)?;
                if !is_trigger
                    && !h.equal_string(&h.get(&opts, "type")?, "market")
                    && h.truthy(&h.get(&opts, "price")?)
                {
                    let price = to_string(&*h, h.get(&opts, "price")?)?;
                    h.assign(&request, "price", price)?;
                }
                if is_trigger {
                    let price = h.get(&opts, "triggerPrice")?;
                    let price = if h.nullish(&price) {
                        h.get(&opts, "price")?
                    } else {
                        price
                    };
                    if !h.is_finite(price.clone())? || h.less_equal_zero(price.clone())? {
                        return Err(construct(
                            &*h,
                            "A positive trigger price is required for ",
                            || h.get(&opts, "type"),
                        )?);
                    }
                    let price = to_string(&*h, price)?;
                    h.assign(&request, "ref_price", price)?;
                    let mapper = h.prepare(&h.root(), "mapStopTriggerType")?;
                    let arg = if h.equal_string(&h.get(&opts, "type")?, "stop_market") {
                        h.get(&opts, "slTriggerType")?
                    } else {
                        h.get(&opts, "tpTriggerType")?
                    };
                    let ref_type = h.call(mapper, vec![arg])?;
                    h.assign(&request, "ref_price_type", ref_type)?;
                }
                if h.truthy(&h.get(&opts, "reduceOnly")?) {
                    h.assign(
                        &request,
                        "exec_inst",
                        h.array(vec![h.string("REDUCE_ONLY")]),
                    )?;
                }
                if h.truthy(&h.get(&opts, "timeInForce")?)
                    && h.equal_string(&h.get(&opts, "type")?, "limit")
                {
                    let mapper = h.prepare(&h.root(), "mapTimeInForce")?;
                    let tif = h.call(mapper, vec![h.get(&opts, "timeInForce")?])?;
                    h.assign(&request, "time_in_force", tif)?;
                }
                let endpoint = h.endpoint(if is_trigger {
                    "ADVANCED_CREATE_ORDER"
                } else {
                    "CREATE_ORDER"
                })?;
                let xhr = h.get(&h.root(), "xhr")?;
                let post = h.prepare(&xhr, "post")?;
                h.post(post, endpoint, request)
            })();
            caught(&*h, started, "Error placing order: ")
        }
    };
    finish(h, begun, "Error placing order: ", true)
}
pub fn cancel_order<H: WriteHost>(
    h: Rc<H>,
    order: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let is_trigger = trigger(&*h, &order)?;
        let xhr = h.get(&h.root(), "xhr")?;
        let post = h.prepare(&xhr, "post")?;
        let endpoint = h.endpoint(if is_trigger {
            "ADVANCED_CANCEL_ORDER"
        } else {
            "CANCEL_ORDER"
        })?;
        let params = if is_trigger {
            h.object(vec![("order_id", h.get(&order, "id")?)])
        } else {
            let symbol = h.get(&order, "symbol")?;
            let id = h.get(&order, "id")?;
            h.object(vec![("instrument_name", symbol), ("order_id", id)])
        };
        h.post(post, endpoint, params)
    })();
    let begun = caught(&*h, begun, "Error canceling order: ");
    finish(h, begun, "Error canceling order: ", false)
}
fn cancel_callback<H: WriteHost>(h: Rc<H>, order: H::Value) -> BoxFuture<H::Value, H::Error> {
    let begun = (|| {
        let method = h.prepare(&h.root(), "cancelOrder")?;
        h.call_async(method, vec![order])
    })();
    Box::pin(async move {
        if let Ok(future) = begun {
            let _ = future.await;
        }
        Ok(h.undefined())
    })
}
pub fn cancel_orders<H: WriteHost>(
    h: Rc<H>,
    orders: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let callback_host = h.clone();
    let begun = h.series(orders, move |order| {
        cancel_callback(callback_host.clone(), order)
    });
    async move {
        begun?.await?;
        Ok(h.undefined())
    }
}
pub fn cancel_symbol_orders<H: WriteHost>(
    h: Rc<H>,
    symbol: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let xhr = h.get(&h.root(), "xhr")?;
        let post = h.prepare(&xhr, "post")?;
        let endpoint = h.endpoint("CANCEL_ALL_ORDERS")?;
        h.post(post, endpoint, h.object(vec![("instrument_name", symbol)]))
    })();
    let begun = caught(&*h, begun, "Error canceling symbol orders: ");
    finish(h, begun, "Error canceling symbol orders: ", false)
}
fn update_value<H: WriteHost>(
    h: &H,
    opts: &H::Value,
    key: &'static str,
) -> Result<H::Value, H::Error> {
    let update = h.get(opts, "update")?;
    let present = h.has(update, key)?;
    let owner = h.get(opts, if present { "update" } else { "order" })?;
    h.get(&owner, key)
}
pub fn update_order<H: WriteHost>(
    h: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let first = h.get(&opts, "order")?;
        let is_trigger = h.equal_string(&h.get(&first, "type")?, "stop_market") || {
            let second = h.get(&opts, "order")?;
            h.equal_string(&h.get(&second, "type")?, "take_profit_market")
        };
        let order = h.get(&opts, "order")?;
        let id = h.get(&order, "id")?;
        let quantity = to_string(&*h, update_value(&*h, &opts, "amount")?)?;
        let mut fields = vec![("order_id", id), ("new_quantity", quantity)];
        if is_trigger {
            let price = to_string(&*h, update_value(&*h, &opts, "price")?)?;
            fields.push(("new_ref_price", price));
        } else {
            let order = h.get(&opts, "order")?;
            let symbol = h.get(&order, "symbol")?;
            fields.push(("instrument_name", symbol));
            let price = to_string(&*h, update_value(&*h, &opts, "price")?)?;
            fields.push(("new_price", price));
        }
        let params = h.object(fields);
        let endpoint = h.endpoint(if is_trigger {
            "ADVANCED_AMEND_ORDER"
        } else {
            "AMEND_ORDER"
        })?;
        let xhr = h.get(&h.root(), "xhr")?;
        let post = h.prepare(&xhr, "post")?;
        h.post(post, endpoint, params)
    })();
    let begun = caught(&*h, begun, "Error updating order: ");
    finish(h, begun, "Error updating order: ", true)
}
enum Batch<F, V, I, E> {
    Waiting { future: F, ids: V, iterator: I },
    Done(Result<V, E>),
}
fn append<H: WriteHost>(h: &H, ids: &H::Value, returned: H::Value) -> Result<(), H::Error> {
    let push = h.prepare(ids, "push")?;
    let args = h.spread(returned)?;
    h.call(push, args)?;
    Ok(())
}
fn batch_step<H: WriteHost>(
    h: &H,
    mut iterator: H::Iterator,
    ids: H::Value,
) -> Batch<H::CallFuture, H::Value, H::Iterator, H::Error> {
    loop {
        let order = match h.next(&mut iterator) {
            Err(e) => return Batch::Done(Err(e)),
            Ok(None) => return Batch::Done(Ok(ids)),
            Ok(Some(o)) => o,
        };
        let begun = (|| {
            let method = h.prepare(&h.root(), "placeOrder")?;
            h.call_async(method, vec![order])
        })();
        match begun {
            Ok(future) => {
                return Batch::Waiting {
                    future,
                    ids,
                    iterator,
                };
            }
            Err(e) => {
                if let Err(e) = log(h, "Error placing order in batch: ", &e) {
                    return Batch::Done(Err(h.close_throw(iterator, e)));
                }
            }
        }
    }
}
pub fn place_orders<H: WriteHost>(
    h: Rc<H>,
    orders: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let assertion = h.prepare(&h.root(), "assertCallerClientOrderIds")?;
        h.call(assertion, vec![orders.clone()])?;
        let ids = h.array(vec![]);
        let iterator = h.iterator(orders)?;
        Ok::<_, H::Error>(batch_step(&*h, iterator, ids))
    })();
    async move {
        let mut state = begun?;
        loop {
            match state {
                Batch::Done(result) => return result,
                Batch::Waiting {
                    future,
                    ids,
                    iterator,
                } => {
                    let result = match future.await {
                        Ok(returned) => append(&*h, &ids, returned),
                        Err(e) => Err(e),
                    };
                    if let Err(error) = result {
                        if let Err(error) = log(&*h, "Error placing order in batch: ", &error) {
                            return Err(h.close_throw(iterator, error));
                        }
                    }
                    state = batch_step(&*h, iterator, ids);
                }
            }
        }
    }
}
fn listen<H: WriteHost>(
    h: &H,
    key: &'static str,
    arg: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let websocket = h.get(&h.root(), "publicWebsocket")?;
    let method = h.prepare(&websocket, key)?;
    h.call(method, vec![arg, callback])
}
pub fn listen_ohlcv<H: WriteHost>(
    h: &H,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, "listenOHLCV", opts, callback)
}
pub fn listen_order_book<H: WriteHost>(
    h: &H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, "listenOrderBook", symbol, callback)
}
