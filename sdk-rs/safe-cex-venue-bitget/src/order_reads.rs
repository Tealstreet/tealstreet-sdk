//! Order REST and recovery bodies over supplied native receiver/HTTP operations.
//! Value clones retain references. The host binds JS language operations and
//! owns started promises; these bodies are polled at their async execution boundary.
use std::{future::Future, pin::Pin};
pub type OrderReadFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait OrderReadsHost {
    type Value: Clone;
    type Error;
    type Iterator;
    fn receiver(&self, name: &str) -> Result<Self::Value, Self::Error>;
    fn set_receiver(&self, name: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Ordinary assignment, distinct from object literal / spread own definitions.
    fn assign(
        &self,
        object: &Self::Value,
        name: &str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn define(
        &self,
        object: &Self::Value,
        name: &str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn index(&self, value: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    /// Native Array.flatMap with the captured mapOrder callable and original
    /// callback arguments, flattening depth, holes, alias and exception behavior.
    fn flat_map(&self, value: Self::Value, mapper: Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn call(
        &self,
        name: &str,
        args: Vec<Self::Value>,
    ) -> Result<OrderReadFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn call_sync(&self, name: &str, args: Vec<Self::Value>) -> Result<(), Self::Error>;
    /// Returns xhr.get's fulfilled Axios `data` value (not the nested payload).
    fn read(
        &self,
        client: Self::Value,
        endpoint: &'static str,
        params: Self::Value,
    ) -> Result<OrderReadFuture<'_, Self::Value, Self::Error>, Self::Error>;
    /// Capture Promise and its all/allSettled method before invoking operands.
    fn promise_function(&self, settled: bool) -> Result<Self::Value, Self::Error>;
    /// Bind original Promise.all / allSettled: fulfillment order, first temporal
    /// rejection and independently host-owned work. Not a Rust polling race.
    fn promise<'a>(
        &'a self,
        function: Self::Value,
        settled: bool,
        values: Vec<OrderReadFuture<'a, Self::Value, Self::Error>>,
    ) -> Result<OrderReadFuture<'a, Self::Value, Self::Error>, Self::Error>;
    fn success(&self, code: &Self::Value) -> Result<bool, Self::Error>;
    fn error_message(
        &self,
        value: &Self::Value,
        fallback: &str,
    ) -> Result<Self::Value, Self::Error>;
    fn emit(&self, value: Self::Value) -> Result<(), Self::Error>;
}
fn s<H: OrderReadsHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn prop<H: OrderReadsHost>(h: &H, v: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    h.property(v, n)
}
fn optional<H: OrderReadsHost>(h: &H, v: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        prop(h, v, n)
    }
}
fn append<H: OrderReadsHost>(h: &H, out: &mut Vec<H::Value>, v: H::Value) -> Result<(), H::Error> {
    let mut i = h.iterator(&v)?;
    while let Some(v) = h.next(&mut i)? {
        out.push(v)
    }
    Ok(())
}
fn mapped<H: OrderReadsHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    let mapper = h.receiver("mapOrder")?;
    h.flat_map(v, mapper)
}
fn classic_params<H: OrderReadsHost>(
    h: &H,
    plan: Option<&str>,
    cursor: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut fields = vec![("productType", h.receiver("apiProductType")?)];
    if let Some(plan) = plan {
        fields.push(("planType", s(h, plan)))
    }
    fields.push(("limit", s(h, "200")));
    let params = h.object(fields);
    if h.truthy(cursor) {
        h.assign(&params, "idLessThan", cursor.clone())?
    }
    Ok(params)
}
async fn classic_pages<H: OrderReadsHost>(h: &H, algo: bool) -> Result<H::Value, H::Error> {
    let mut all = vec![];
    let mut end = h.undefined();
    let mut iterations = 0;
    while iterations < 20 {
        let params = classic_params(h, if algo { Some("profit_loss") } else { None }, &end)?;
        let response = h
            .read(
                h.receiver("xhr")?,
                if algo { "ALGO_ORDERS" } else { "ORDERS" },
                params,
            )?
            .await?;
        let data = prop(h, &response, "data")?;
        if !h.is_array(&optional(h, &data, "entrustedList")?)
            || h.equal(
                &prop(h, &prop(h, &data, "entrustedList")?, "length")?,
                &h.number(0.),
            )
        {
            break;
        }
        append(h, &mut all, prop(h, &data, "entrustedList")?)?;
        if !h.truthy(&prop(h, &data, "endId")?) || h.equal(&prop(h, &data, "endId")?, &end) {
            break;
        }
        end = prop(h, &data, "endId")?;
        iterations += 1;
    }
    mapped(h, h.array(all))
}
// ts: BitgetExchange.fetchOrders
pub async fn fetch_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    h.call("ensureAccountTypeDetected", vec![])?.await?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        return h.call("fetchUtaOrders", vec![])?.await;
    }
    let normal = h.call("fetchNormalOrders", vec![])?.await?;
    let algo = h.call("fetchAlgoOrders", vec![])?.await?;
    let trailing = h.call("fetchTrailingStopOrders", vec![])?.await?;
    let mut out = vec![];
    append(h, &mut out, normal)?;
    append(h, &mut out, algo)?;
    append(h, &mut out, trailing)?;
    Ok(h.array(out))
}
// ts: BitgetExchange.fetchNormalOrders
pub async fn fetch_normal_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    h.call("ensureAccountTypeDetected", vec![])?.await?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        return h.call("fetchUtaNormalOrders", vec![])?.await;
    }
    classic_pages(h, false).await
}
// ts: BitgetExchange#fetchUtaOrders
pub async fn fetch_uta_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    let function = h.promise_function(false)?;
    let normal = h.call("fetchUtaNormalOrders", vec![])?;
    let strategy = h.call("fetchUtaStrategyOrders", vec![])?;
    let values = h.promise(function, false, vec![normal, strategy])?.await?;
    let normal = h.index(&values, 0)?;
    let strategy = h.index(&values, 1)?;
    let mut out = vec![];
    append(h, &mut out, normal)?;
    append(h, &mut out, strategy)?;
    Ok(h.array(out))
}
// ts: BitgetExchange#fetchUtaNormalOrders
pub async fn fetch_uta_normal_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    let mut all = vec![];
    let mut cursor = h.undefined();
    for _ in 0..20 {
        let params = h.object(vec![
            ("category", h.receiver("apiUtaCategory")?),
            ("limit", s(h, "100")),
        ]);
        if h.truthy(&cursor) {
            h.assign(&params, "cursor", cursor.clone())?
        }
        let data = h.read(h.receiver("xhr")?, "UTA_ORDERS", params)?.await?;
        if !h.success(&prop(h, &data, "code")?)? {
            h.emit(h.error_message(&data, "Bitget UTA open orders request failed")?)?;
            return mapped(h, h.array(all));
        }
        let payload = prop(h, &data, "data")?;
        let first = optional(h, &payload, "list")?;
        let list = if h.is_array(&first) {
            prop(h, &prop(h, &data, "data")?, "list")?
        } else {
            h.array(vec![])
        };
        if !h.truthy(&prop(h, &list, "length")?) {
            break;
        }
        append(h, &mut all, list)?;
        let next = optional(h, &prop(h, &data, "data")?, "cursor")?;
        if !h.truthy(&next) || h.equal(&next, &cursor) {
            break;
        }
        cursor = next;
    }
    mapped(h, h.array(all))
}
// ts: BitgetExchange#fetchUtaStrategyOrders
pub async fn fetch_uta_strategy_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    let client = h.receiver("xhr")?;
    let params = h.object(vec![
        ("category", h.receiver("apiUtaCategory")?),
        ("type", s(h, "tpsl")),
    ]);
    let data = h.read(client, "UTA_STRATEGY_ORDERS", params)?.await?;
    if !h.success(&prop(h, &data, "code")?)? {
        h.emit(h.error_message(&data, "Bitget UTA strategy orders request failed")?)?;
        return Ok(h.array(vec![]));
    }
    let first = prop(h, &data, "data")?;
    let rows = if h.is_array(&first) {
        prop(h, &data, "data")?
    } else {
        let payload = prop(h, &data, "data")?;
        if h.is_array(&optional(h, &payload, "list")?) {
            prop(h, &prop(h, &data, "data")?, "list")?
        } else {
            h.array(vec![])
        }
    };
    mapped(h, rows)
}
// ts: BitgetExchange.fetchAlgoOrders
pub async fn fetch_algo_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    classic_pages(h, true).await
}
// ts: BitgetExchange.fetchTrailingStopOrders
pub async fn fetch_trailing_stop_orders<H: OrderReadsHost>(h: &H) -> Result<H::Value, H::Error> {
    let mut all = vec![];
    let mut end = h.undefined();
    let mut iterations = 0;
    while iterations < 20 {
        let params = classic_params(h, Some("track_plan"), &end)?;
        let response = h.read(h.receiver("xhr")?, "ALGO_ORDERS", params)?.await?;
        let data = prop(h, &response, "data")?;
        if h.is_array(&data) {
            return mapped(h, data);
        } else if h.truthy(&optional(h, &data, "entrustedList")?) {
            let orders = prop(h, &data, "entrustedList")?;
            if h.equal(&prop(h, &orders, "length")?, &h.number(0.)) {
                break;
            }
            append(h, &mut all, orders)?;
            if !h.truthy(&prop(h, &data, "endId")?) || h.equal(&prop(h, &data, "endId")?, &end) {
                break;
            }
            end = prop(h, &data, "endId")?;
        } else {
            break;
        }
        iterations += 1;
    }
    mapped(h, h.array(all))
}
// ts: BitgetExchange#reconcilePrivateStateFromRest
pub async fn reconcile_private_state_from_rest<H: OrderReadsHost>(h: &H) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("isDisposed")?)
        || !h.truthy(&h.receiver("isPrivateStreamDegraded")?)
        || h.truthy(&h.receiver("privateStreamDegradedReconcileRunning")?)
    {
        return Ok(());
    }
    h.set_receiver("privateStreamDegradedReconcileRunning", h.boolean(true))?;
    let result = async {
        let function = h.promise_function(true)?;
        let positions = h.call("fetchPositions", vec![])?;
        let orders = h.call("fetchOrders", vec![])?;
        let balance = h.call(
            "fetchBalance",
            vec![h.object(vec![("preferFast", h.boolean(true))])],
        )?;
        let result = h
            .promise(function, true, vec![positions, orders, balance])?
            .await?;
        let positions = h.index(&result, 0)?;
        let orders = h.index(&result, 1)?;
        let balance = h.index(&result, 2)?;
        if h.truthy(&h.receiver("isDisposed")?)
            || !h.truthy(&h.receiver("isPrivateStreamDegraded")?)
        {
            return Ok(());
        }
        let loaded = h.spread(&h.receiver("store.loaded")?)?;
        let update = h.object(vec![]);
        if h.equal(&prop(h, &positions, "status")?, &s(h, "fulfilled")) {
            h.call_sync(
                "store.addOrUpdatePositionsFull",
                vec![prop(h, &positions, "value")?],
            )?;
            h.assign(&loaded, "positions", h.boolean(true))?;
        }
        if h.equal(&prop(h, &orders, "status")?, &s(h, "fulfilled")) {
            h.assign(&update, "orders", prop(h, &orders, "value")?)?;
            h.assign(&loaded, "orders", h.boolean(true))?;
        }
        if h.equal(&prop(h, &balance, "status")?, &s(h, "fulfilled")) {
            h.assign(&update, "balance", prop(h, &balance, "value")?)?;
            h.assign(&loaded, "balance", h.boolean(true))?;
        }
        let patch = h.spread(&update)?;
        h.define(&patch, "loaded", loaded)?;
        h.call_sync("store.update", vec![patch])?;
        Ok(())
    }
    .await;
    h.set_receiver("privateStreamDegradedReconcileRunning", h.boolean(false))?;
    h.call_sync("schedulePrivateStreamDegradedReconcile", vec![])?;
    result
}
