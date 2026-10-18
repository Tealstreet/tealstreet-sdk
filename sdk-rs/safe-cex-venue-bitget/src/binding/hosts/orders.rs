//! Concrete host traits and target adapters for this group of ported bodies:
//! order reads, cancellation, order mappers, private-stream predicates and caches.
use crate::binding::{
    globals::arg,
    promise::{JsFuture, Promise, await_js},
    rt::{Rt, Shared},
    value::{Js, Thrown, from_typed, to_json},
};
use crate::{
    cancellation::{
        self, CancelArgument, CancelFuture, CancelGroups, CancelKey, CancelOrders, CancelPayload,
        CancellationHost,
    },
    order_lifecycle::OrderLifecycleCaches,
    order_reads::{self, OrderReadFuture, OrderReadsHost},
    orders, stream_predicates,
};
use indexmap::IndexMap;
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    rc::{Rc, Weak},
    task::Poll,
};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// `value?.key`
fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}

/// Optional JSON projection: a missing (undefined) argument stays `None`.
fn json_arg(value: &Js) -> Result<Option<serde_json::Value>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        to_json(value).map(Some)
    }
}

fn to_orders(value: &Js) -> Result<CancelOrders<Js>, Thrown> {
    value
        .array_values()?
        .map(|items| Rc::new(RefCell::new(items)))
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", value.to_display())))
}

fn orders_value(orders: &CancelOrders<Js>) -> Js {
    Js::array(orders.borrow().clone())
}

fn settled_future(rt: &Rt, value: Js) -> JsFuture<'static> {
    Box::pin(await_js(&rt.exchange_runtime(), value))
}

/// Invoke the captured Promise combinator over pending handles that the returned
/// future settles as each operand completes; operand work is already runtime-owned.
fn combine<'a>(rt: &Rt, function: Js, inputs: Vec<JsFuture<'a>>) -> Result<JsFuture<'a>, Thrown> {
    let runtime = rt.exchange_runtime();
    let pending: Vec<Promise> = inputs.iter().map(|_| Promise::pending(&runtime)).collect();
    let handles = Js::array(pending.iter().cloned().map(Js::Promise).collect());
    let combined = function.call(rt.global("Promise")?, vec![handles])?;
    let mut inputs: Vec<Option<JsFuture<'a>>> = inputs.into_iter().map(Some).collect();
    let mut outcome = settled_future(rt, combined);
    Ok(Box::pin(poll_fn(move |cx| {
        for (slot, promise) in inputs.iter_mut().zip(&pending) {
            if let Some(future) = slot
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                promise.settle(result);
                *slot = None;
            }
        }
        outcome.as_mut().poll(cx)
    })))
}

fn promise_member(rt: &Rt, name: &str) -> Result<Js, Thrown> {
    rt.global("Promise")?.get(name)
}

/// `this.<path>(...args)` started now; the future awaits its (possibly promise) result.
fn call_now(rt: &Rt, path: &str, args: Vec<Js>) -> Result<JsFuture<'static>, Thrown> {
    Ok(settled_future(rt, rt.call_path(path, args)?))
}

impl CancellationHost for Rt {
    type Error = Thrown;
    type Order = Js;
    type Value = Js;
    fn undefined_order() -> Js {
        Js::Undefined
    }
    fn ensure_account_type(&self) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        let future = call_now(self, "ensureAccountTypeDetected", vec![])?;
        Ok(Box::pin(async move { future.await.map(|_| ()) }))
    }
    fn uses_uta(&self) -> Result<bool, Thrown> {
        Ok(self.path("usesUtaRouting")?.truthy())
    }
    fn promise_all_function(&self) -> Result<Js, Thrown> {
        promise_member(self, "all")
    }
    fn promise_all<'a>(
        &'a self,
        function: Js,
        normal: CancelFuture<'a, CancelOrders<Js>, Thrown>,
        strategy: CancelFuture<'a, CancelOrders<Js>, Thrown>,
    ) -> Result<CancelFuture<'a, (CancelOrders<Js>, CancelOrders<Js>), Thrown>, Thrown> {
        let as_js = |f: CancelFuture<'a, CancelOrders<Js>, Thrown>| -> JsFuture<'a> {
            Box::pin(async move { f.await.map(|orders| orders_value(&orders)) })
        };
        let combined = combine(self, function, vec![as_js(normal), as_js(strategy)])?;
        Ok(Box::pin(async move {
            let values = combined.await?;
            Ok((to_orders(&values.get("0")?)?, to_orders(&values.get("1")?)?))
        }))
    }
    fn fetch_uta_normal(&self) -> Result<CancelFuture<'_, CancelOrders<Js>, Thrown>, Thrown> {
        let future = call_now(self, "fetchUtaNormalOrders", vec![])?;
        Ok(Box::pin(async move { to_orders(&future.await?) }))
    }
    fn fetch_uta_strategy(&self) -> Result<CancelFuture<'_, CancelOrders<Js>, Thrown>, Thrown> {
        let future = call_now(self, "fetchUtaStrategyOrders", vec![])?;
        Ok(Box::pin(async move { to_orders(&future.await?) }))
    }
    fn cancel_normal(
        &self,
        orders: CancelOrders<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        delegate(self, "cancelNormalOrders", &orders)
    }
    fn cancel_algo(
        &self,
        orders: CancelOrders<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        delegate(self, "cancelAlgoOrders", &orders)
    }
    fn cancel_uta_normal(
        &self,
        orders: CancelOrders<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        delegate(self, "cancelUtaNormalOrders", &orders)
    }
    fn cancel_uta_strategy(
        &self,
        orders: CancelOrders<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        delegate(self, "cancelUtaStrategyOrders", &orders)
    }
    fn cancel_orders(
        &self,
        orders: CancelOrders<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        delegate(self, "cancelOrders", &orders)
    }
    fn virtual_tpsl(&self, order: &Js) -> Result<bool, Thrown> {
        Ok(self
            .call_path("isVirtualTPSLOrder", vec![order.clone()])?
            .truthy())
    }
    fn algo(&self, order: &Js) -> Result<bool, Thrown> {
        Ok(self.call_path("isAlgoOrder", vec![order.clone()])?.truthy())
    }
    fn spread(&self, order: &Js) -> Result<Js, Thrown> {
        order.spread_object()
    }
    /// `order.id.split('__')`: every normalizing site requires a string id.
    fn id(&self, order: &Js) -> Result<CancelKey, Thrown> {
        match order.get("id")? {
            Js::String(s) => Ok(s.to_vec()),
            nullish @ (Js::Undefined | Js::Null) => Err(Thrown::type_error(&format!(
                "Cannot read properties of {} (reading 'split')",
                nullish.to_display()
            ))),
            _ => Err(Thrown::type_error("o.id.split is not a function")),
        }
    }
    fn with_id(&self, spread: Js, id: CancelKey) -> Js {
        let _ = spread.define("id", Js::String(id.into()));
        spread
    }
    fn symbol(&self, order: &Js) -> Result<CancelKey, Thrown> {
        Ok(order.get("symbol")?.to_js_string()?.to_vec())
    }
    fn info_order_id(&self, order: &Js) -> Result<Js, Thrown> {
        optional(&order.get("info")?, "orderId")
    }
    fn info_client_oid(&self, order: &Js) -> Result<Js, Thrown> {
        optional(&order.get("info")?, "clientOid")
    }
    fn truthy(value: &Js) -> bool {
        value.truthy()
    }
    fn is_undefined(value: &Js) -> bool {
        value.is_undefined()
    }
    fn undefined() -> Js {
        Js::Undefined
    }
    fn key_value(key: CancelKey) -> Js {
        Js::String(key.into())
    }
    fn equals_key(value: &Js, key: &CancelKey) -> bool {
        matches!(value, Js::String(s) if s[..] == key[..])
    }
    fn product_type(&self) -> Result<Js, Thrown> {
        self.path("apiProductType")
    }
    fn margin_coin(&self) -> Result<Js, Thrown> {
        self.path("apiMarginCoin")
    }
    fn uta_category(&self) -> Result<Js, Thrown> {
        self.path("apiUtaCategory")
    }
    fn plan_type(&self, order: &Js) -> Result<Js, Thrown> {
        self.call_path("getCancelPlanType", vec![order.clone()])
    }
    fn margin_coin_for_symbol(&self, symbol: &CancelKey) -> Result<Js, Thrown> {
        self.call_path("getMarginCoinForSymbol", vec![Js::utf16(symbol)])
    }
    /// `const [planType] = this.selectPlanTypeAndAmount(order, this.store.positions)`
    fn inverse_plan_type(&self, order: &Js) -> Result<Js, Thrown> {
        let (owner, select) = self.owner_and_member("selectPlanTypeAndAmount")?;
        let positions = self.path("store.positions")?;
        if !select.is_callable() {
            return Err(Thrown::type_error(
                "this.selectPlanTypeAndAmount is not a function",
            ));
        }
        let selected = select.call(owner, vec![order.clone(), positions])?;
        let iterator = js_iterator(&selected)?;
        let value = iterator.next()?.unwrap_or(Js::Undefined);
        iterator.0.close()?;
        Ok(value)
    }
    fn market_bases(&self) -> Result<Vec<CancelKey>, Thrown> {
        let markets = self.path("store.markets")?;
        let Some(items) = markets.array_items()? else {
            return Err(Thrown::type_error(
                "this.store.markets.map is not a function",
            ));
        };
        items
            .into_iter()
            .flatten()
            .map(|market| Ok(market.get("base")?.to_js_string()?.to_vec()))
            .collect()
    }
    fn store_orders(&self) -> Result<CancelOrders<Js>, Thrown> {
        let orders = self.path("store.orders")?;
        if !orders.is_array() {
            return Err(Thrown::type_error(
                "this.store.orders.filter is not a function",
            ));
        }
        to_orders(&orders)
    }
    fn inverse_group_prepare(
        &self,
        groups: &mut CancelGroups<Js>,
        key: &CancelKey,
    ) -> Result<Js, Thrown> {
        // Inherited Object.prototype members are truthy and have no `push`.
        if OBJECT_PROTOTYPE_KEYS
            .iter()
            .any(|k| k.encode_utf16().eq(key.iter().copied()))
        {
            return Ok(Js::Undefined);
        }
        if !groups.iter().any(|(k, _)| k == key) {
            groups.push((key.clone(), vec![]));
        }
        Ok(Js::Bool(true))
    }
    fn inverse_group_push(
        &self,
        groups: &mut CancelGroups<Js>,
        key: CancelKey,
        order: Js,
        push: Js,
    ) -> Result<(), Thrown> {
        if push.is_undefined() {
            return Err(Thrown::type_error("acc[key].push is not a function"));
        }
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, items)) => items.push(order),
            None => groups.push((key, vec![order])),
        }
        Ok(())
    }
    fn post(
        &self,
        endpoint: &'static str,
        payload: CancelPayload<Js>,
    ) -> Result<CancelFuture<'_, (), Thrown>, Thrown> {
        let body = Js::object(vec![]);
        for (name, value) in payload {
            let value = match value {
                CancelArgument::Value(v) => v,
                CancelArgument::String(s) => Js::String(s.into()),
                CancelArgument::OrderIds(ids) => Js::array(
                    ids.into_iter()
                        .map(|id| Js::object(vec![("orderId", Js::String(id.into()))]))
                        .collect(),
                ),
            };
            body.define(name, value)?;
        }
        let client = self.path("unlimitedXHR")?;
        let response = client.call_method("post", vec![Js::str(endpoint), body])?;
        Ok(Box::pin(async move {
            await_js(&self.exchange_runtime(), response)
                .await
                .map(|_| ())
        }))
    }
    fn schedule_write_check(&self) -> Result<(), Thrown> {
        self.call_path("schedulePrivateStreamWriteCheck", vec![])
            .map(|_| ())
    }
    /// `err?.response?.data?.msg || err?.message`
    fn error_message(&self, error: &Thrown) -> Result<Js, Thrown> {
        let msg = optional(
            &optional(&optional(&error.value, "response")?, "data")?,
            "msg",
        )?;
        if msg.truthy() {
            Ok(msg)
        } else {
            optional(&error.value, "message")
        }
    }
    fn emit_error(&self, message: Js) -> Result<(), Thrown> {
        self.call_path("emitAndLogError", vec![message]).map(|_| ())
    }
}

const OBJECT_PROTOTYPE_KEYS: [&str; 12] = [
    "__proto__",
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
];

fn delegate<'a>(
    rt: &'a Rt,
    path: &str,
    orders: &CancelOrders<Js>,
) -> Result<CancelFuture<'a, (), Thrown>, Thrown> {
    let future = call_now(rt, path, vec![orders_value(orders)])?;
    Ok(Box::pin(async move { future.await.map(|_| ()) }))
}

/// Array/string iteration protocol (`...value`, destructuring) over the live value.
pub(crate) struct JsIterator(safe_cex_exchange::js_runtime::iterator::IteratorRecord);
impl JsIterator {
    fn next(&self) -> Result<Option<Js>, Thrown> {
        self.0.step_value()
    }
}
fn js_iterator(value: &Js) -> Result<JsIterator, Thrown> {
    Ok(JsIterator(
        safe_cex_exchange::js_runtime::iterator::IteratorRecord::get(value)?,
    ))
}

impl OrderReadsHost for Rt {
    type Value = Js;
    type Error = Thrown;
    type Iterator = JsIterator;
    fn receiver(&self, name: &str) -> Result<Js, Thrown> {
        self.path(name)
    }
    fn set_receiver(&self, name: &str, value: Js) -> Result<(), Thrown> {
        self.set_path(name, value)
    }
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn spread(&self, value: &Js) -> Result<Js, Thrown> {
        value.spread_object()
    }
    fn assign(&self, object: &Js, name: &str, value: Js) -> Result<(), Thrown> {
        object.set(name, value)
    }
    fn define(&self, object: &Js, name: &str, value: Js) -> Result<(), Thrown> {
        object.define(name, value)
    }
    fn iterator(&self, value: &Js) -> Result<JsIterator, Thrown> {
        js_iterator(value)
    }
    fn next(&self, iterator: &mut JsIterator) -> Result<Option<Js>, Thrown> {
        iterator.next()
    }
    fn index(&self, value: &Js, index: usize) -> Result<Js, Thrown> {
        value.get(&index.to_string())
    }
    /// Array.prototype.flatMap(mapper): depth 1, holes skipped, mapper(value, index, array).
    fn flat_map(&self, value: Js, mapper: Js) -> Result<Js, Thrown> {
        let Js::Array(array) = &value else {
            return Err(Thrown::type_error(&format!(
                "{}.flatMap is not a function",
                value.type_of()
            )));
        };
        let len = array.len();
        if !mapper.is_callable() {
            return Err(Thrown::type_error(&format!(
                "flatMap mapper function is not callable ({})",
                mapper.to_display()
            )));
        }
        let mut out: Vec<Option<Js>> = vec![];
        for i in 0..len {
            let key = i.to_string();
            if !value.has_index(i) {
                continue;
            }
            let element = value.get(&key)?;
            let mapped = mapper.call(
                Js::Undefined,
                vec![element, Js::Number(i as f64), value.clone()],
            )?;
            match mapped.array_items()? {
                Some(items) => out.extend(items.into_iter().filter(Option::is_some)),
                None => out.push(Some(mapped)),
            }
        }
        Ok(Js::holey_array(out))
    }
    fn call(&self, name: &str, args: Vec<Js>) -> Result<OrderReadFuture<'_, Js, Thrown>, Thrown> {
        call_now(self, name, args)
    }
    fn call_sync(&self, name: &str, args: Vec<Js>) -> Result<(), Thrown> {
        self.call_path(name, args).map(|_| ())
    }
    /// `this.xhr.get(ENDPOINTS[endpoint], { params })` resolved to `response.data`.
    fn read(
        &self,
        client: Js,
        endpoint: &'static str,
        params: Js,
    ) -> Result<OrderReadFuture<'_, Js, Thrown>, Thrown> {
        let url = self.global("ENDPOINTS")?.get(endpoint)?;
        let get = client.get("get")?;
        if !get.is_callable() {
            return Err(Thrown::type_error("this.xhr.get is not a function"));
        }
        let response = get.call(client, vec![url, Js::object(vec![("params", params)])])?;
        Ok(Box::pin(async move {
            await_js(&self.exchange_runtime(), response)
                .await?
                .get("data")
        }))
    }
    fn promise_function(&self, settled: bool) -> Result<Js, Thrown> {
        promise_member(self, if settled { "allSettled" } else { "all" })
    }
    fn promise<'a>(
        &'a self,
        function: Js,
        _settled: bool,
        values: Vec<OrderReadFuture<'a, Js, Thrown>>,
    ) -> Result<OrderReadFuture<'a, Js, Thrown>, Thrown> {
        combine(self, function, values)
    }
    fn success(&self, code: &Js) -> Result<bool, Thrown> {
        Ok(self
            .global("isBitgetSuccessCode")?
            .call(Js::Undefined, vec![code.clone()])?
            .truthy())
    }
    fn error_message(&self, value: &Js, fallback: &str) -> Result<Js, Thrown> {
        self.global("getBitgetApiErrorMessage")?
            .call(Js::Undefined, vec![value.clone(), Js::str(fallback)])
    }
    fn emit(&self, value: Js) -> Result<(), Thrown> {
        self.call_path("emitAndLogError", vec![value]).map(|_| ())
    }
}

/// bitget.exchange.ts module helpers the order bodies resolve by name.
pub(crate) fn register(globals: &Js, _weak: Weak<Shared>) {
    let _ = globals.define(
        "isBitgetSuccessCode",
        Js::function("isBitgetSuccessCode", |_, a| {
            let code = arg(&a, 0);
            let code = if code.is_nullish() {
                Js::Number(0.0)
            } else {
                code
            };
            Ok(Js::Bool(
                safe_cex_num::parse_int(&text(&code)?, Some(10)) == 0.0,
            ))
        }),
    );
    let _ = globals.define(
        "getBitgetApiErrorMessage",
        Js::function("getBitgetApiErrorMessage", |_, a| {
            let msg = optional(&arg(&a, 0), "msg")?;
            Ok(if msg.truthy() { msg } else { arg(&a, 1) })
        }),
    );
}

/// The receiver a bound member runs on; the runtime is held weakly to avoid a cycle.
#[derive(Clone)]
struct Bound {
    shared: Weak<Shared>,
    this: Js,
}

impl Bound {
    fn new(rt: &Rt) -> Self {
        Self {
            shared: Rc::downgrade(&rt.shared),
            this: rt.this.clone(),
        }
    }
    fn rt(&self) -> Result<Rt, Thrown> {
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Bitget runtime disposed"))?;
        Ok(Rt {
            shared,
            this: self.this.clone(),
        })
    }
}

fn sync_member(
    rt: &Rt,
    name: &str,
    body: impl Fn(&Rt, Vec<Js>) -> Result<Js, Thrown> + 'static,
) -> Js {
    let bound = Bound::new(rt);
    Js::function(name, move |_, args| body(&bound.rt()?, args))
}

fn async_member<F, Fut>(rt: &Rt, name: &str, body: F) -> Js
where
    F: Fn(Rt, Vec<Js>) -> Fut + 'static,
    Fut: Future<Output = Result<Js, Thrown>> + 'static,
{
    let bound = Bound::new(rt);
    Js::function(name, move |_, args| {
        let rt = bound.rt()?;
        Ok(rt.spawn(Box::pin(body(rt.clone(), args))))
    })
}

fn undefined_after(result: Result<(), Thrown>) -> Result<Js, Thrown> {
    result.map(|()| Js::Undefined)
}

fn api_product_type(rt: &Rt) -> Result<String, Thrown> {
    match rt.path("apiProductType")? {
        Js::String(s) => Ok(String::from_utf16_lossy(&s)),
        _ => Err(Thrown::type_error(
            "this.apiProductType.toUpperCase is not a function",
        )),
    }
}

fn map_order(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let product = api_product_type(rt)?;
    let raw = arg(&args, 0);
    let input = if raw.is_undefined() {
        None
    } else {
        Some(safe_cex_exchange::js_runtime::value::to_retained_input(
            &raw,
        )?)
    };
    from_typed(&orders::map_order(input.as_ref(), &product, || {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
    })?)
}

fn map_inverse_order(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let product = api_product_type(rt)?;
    let raw = arg(&args, 0);
    let input = if raw.is_undefined() {
        None
    } else {
        Some(safe_cex_exchange::js_runtime::value::to_retained_input(
            &raw,
        )?)
    };
    let store = rt.shared.store.clone();
    let mapped = orders::map_inverse_order(
        input.as_ref(),
        &product,
        |symbol| {
            Ok(store
                .markets_by_symbol()?
                .get(symbol)
                .and_then(|m| m.is_inverse))
        },
        || safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
    )?;
    from_typed(&mapped)
}

/// The receiver's two lifecycle Maps are the state other socket bodies read
/// (`terminalOrderTombstones.get`), so each call loads them, runs the body and
/// writes the result back into the same Map objects.
fn with_lifecycle(rt: &Rt, body: impl FnOnce(&mut OrderLifecycleCaches)) -> Result<(), Thrown> {
    let map = |name: &str| -> Result<
        Rc<RefCell<safe_cex_exchange::js_runtime::collections::Collection<(Js, Js)>>>,
        Thrown,
    > {
        match rt.this.get(name)? {
            Js::Map(m) => Ok(m),
            nullish @ (Js::Undefined | Js::Null) => Err(Thrown::type_error(&format!(
                "Cannot read properties of {} (reading 'set')",
                nullish.to_display()
            ))),
            _ => Err(Thrown::type_error(&format!(
                "this.{name}.set is not a function"
            ))),
        }
    };
    let tombstones = map("terminalOrderTombstones")?;
    let open = map("recentWsOpenOrders")?;
    let load = |m: &Rc<
        RefCell<safe_cex_exchange::js_runtime::collections::Collection<(Js, Js)>>,
    >|
     -> Result<IndexMap<String, f64>, Thrown> {
        let entries = m.borrow().clone();
        entries
            .iter()
            .map(|(k, v)| Ok((text(k)?, v.to_number()?)))
            .collect()
    };
    let mut caches = OrderLifecycleCaches {
        terminal_order_tombstones: load(&tombstones)?,
        recent_ws_open_orders: load(&open)?,
    };
    body(&mut caches);
    let store =
        |m: &Rc<RefCell<safe_cex_exchange::js_runtime::collections::Collection<(Js, Js)>>>,
         next: &IndexMap<String, f64>|
         -> Result<(), Thrown> {
            let previous = m.borrow().clone();
            let mut out = Vec::with_capacity(next.len());
            for (key, expires) in next {
                // Unchanged entries keep their original key/value values.
                let kept = previous.iter().find(|(k, v)| {
                    text(k).is_ok_and(|k| &k == key)
                        && v.to_number()
                            .is_ok_and(|v| v.to_bits() == expires.to_bits())
                });
                out.push(
                    kept.cloned()
                        .unwrap_or_else(|| (Js::str(key), Js::Number(*expires))),
                );
            }
            let mut slots = m.borrow_mut();
            slots.retain(|(key, _)| out.iter().any(|(next, _)| next.same_value_zero(key)));
            for (key, value) in out {
                if let Some(slot) = slots
                    .iter_mut()
                    .find(|(existing, _)| existing.same_value_zero(&key))
                {
                    slot.1 = value;
                } else {
                    slots.push((key, value));
                }
            }
            Ok(())
        };
    store(&tombstones, &caches.terminal_order_tombstones)?;
    store(&open, &caches.recent_ws_open_orders)
}

fn order_id_arg(args: &[Js]) -> Result<String, Thrown> {
    text(&arg(args, 0))
}

pub(crate) fn bind(
    rt: &Rt,
    owner: &str,
    name: &str,
    _targets: &[&str],
) -> Result<Option<Js>, Thrown> {
    let member = match (owner, name) {
        ("BitgetExchange", "reconcilePrivateStateFromRest") => {
            async_member(rt, name, |rt, _| async move {
                undefined_after(order_reads::reconcile_private_state_from_rest(&rt).await)
            })
        }
        ("BitgetExchange", "fetchOrders") => async_member(rt, name, |rt, _| async move {
            order_reads::fetch_orders(&rt).await
        }),
        ("BitgetExchange", "fetchNormalOrders") => async_member(rt, name, |rt, _| async move {
            order_reads::fetch_normal_orders(&rt).await
        }),
        ("BitgetExchange", "fetchUtaOrders") => async_member(rt, name, |rt, _| async move {
            order_reads::fetch_uta_orders(&rt).await
        }),
        ("BitgetExchange", "fetchUtaNormalOrders") => async_member(rt, name, |rt, _| async move {
            order_reads::fetch_uta_normal_orders(&rt).await
        }),
        ("BitgetExchange", "fetchUtaStrategyOrders") => {
            async_member(rt, name, |rt, _| async move {
                order_reads::fetch_uta_strategy_orders(&rt).await
            })
        }
        ("BitgetExchange", "fetchAlgoOrders") => async_member(rt, name, |rt, _| async move {
            order_reads::fetch_algo_orders(&rt).await
        }),
        ("BitgetExchange", "fetchTrailingStopOrders") => {
            async_member(rt, name, |rt, _| async move {
                order_reads::fetch_trailing_stop_orders(&rt).await
            })
        }
        ("BitgetExchange", "cancelAllOrders") => async_member(rt, name, |rt, _| async move {
            undefined_after(cancellation::cancel_all_orders(&rt).await)
        }),
        ("BitgetInverseExchange", "cancelAllOrders") => {
            async_member(rt, name, |rt, _| async move {
                undefined_after(cancellation::inverse_cancel_all_orders(&rt).await)
            })
        }
        ("BitgetExchange", "cancelOrders") => async_member(rt, name, |rt, args| async move {
            // lodash partition treats a nullish collection as empty.
            let orders = match arg(&args, 0) {
                v if v.is_nullish() => Rc::new(RefCell::new(vec![])),
                v => to_orders(&v)?,
            };
            undefined_after(cancellation::cancel_orders(&rt, orders).await)
        }),
        ("BitgetExchange", "cancelNormalOrders") => async_member(rt, name, |rt, args| async move {
            let orders = to_orders(&arg(&args, 0))?;
            undefined_after(cancellation::cancel_normal_orders(&rt, orders).await)
        }),
        ("BitgetExchange", "cancelUtaNormalOrders") => {
            async_member(rt, name, |rt, args| async move {
                let orders = to_orders(&arg(&args, 0))?;
                undefined_after(cancellation::cancel_uta_normal_orders(&rt, orders).await)
            })
        }
        ("BitgetExchange", "cancelAlgoOrders") => async_member(rt, name, |rt, args| async move {
            let orders = to_orders(&arg(&args, 0))?;
            undefined_after(cancellation::cancel_algo_orders(&rt, orders).await)
        }),
        ("BitgetExchange", "cancelSymbolOrders") => async_member(rt, name, |rt, args| async move {
            let symbol = arg(&args, 0).to_js_string()?.to_vec();
            undefined_after(cancellation::cancel_symbol_orders(&rt, symbol).await)
        }),
        ("BitgetInverseExchange", "cancelNormalOrders") => {
            async_member(rt, name, |rt, args| async move {
                let orders = to_orders(&arg(&args, 0))?;
                undefined_after(cancellation::inverse_cancel_normal_orders(&rt, orders).await)
            })
        }
        ("BitgetInverseExchange", "cancelAlgoOrders") => {
            async_member(rt, name, |rt, args| async move {
                let orders = to_orders(&arg(&args, 0))?;
                undefined_after(cancellation::inverse_cancel_algo_orders(&rt, orders).await)
            })
        }
        ("BitgetExchange", "mapOrder") => sync_member(rt, name, map_order),
        ("BitgetInverseExchange", "mapOrder") => sync_member(rt, name, map_inverse_order),
        ("BitgetPrivateWebsocket", "normalizeOrderStatus") => sync_member(rt, name, |_, args| {
            let value = json_arg(&arg(&args, 0))?;
            Ok(Js::str(&stream_predicates::normalize_order_status(
                value.as_ref(),
            )?))
        }),
        ("BitgetPrivateWebsocket", "getRawOrderStatus") => sync_member(rt, name, |_, args| {
            let order = json_arg(&arg(&args, 0))?;
            Ok(Js::str(&stream_predicates::get_raw_order_status(
                order.as_ref(),
            )?))
        }),
        ("BitgetPrivateWebsocket", "normalizeChannel") => sync_member(rt, name, |_, args| {
            let value = json_arg(&arg(&args, 0))?;
            Ok(Js::str(&stream_predicates::normalize_channel(
                value.as_ref(),
            )?))
        }),
        ("BitgetPrivateWebsocket", "isTerminalFillStatus") => sync_member(rt, name, |_, args| {
            // Set.has: only an equal string is a member.
            Ok(Js::Bool(match arg(&args, 0) {
                Js::String(s) => {
                    stream_predicates::is_terminal_fill_status(&String::from_utf16_lossy(&s))
                }
                _ => false,
            }))
        }),
        ("BitgetPrivateWebsocket", "pruneOrderLifecycleCaches") => {
            sync_member(rt, name, |rt, args| {
                let now = arg(&args, 0).to_number()?;
                undefined_after(with_lifecycle(rt, |c| c.prune_order_lifecycle_caches(now)))
            })
        }
        ("BitgetPrivateWebsocket", "markOrderAsTerminal") => sync_member(rt, name, |rt, args| {
            let id = order_id_arg(&args)?;
            let clock = rt.clone();
            undefined_after(with_lifecycle(rt, |c| {
                c.mark_order_as_terminal(&id, || clock.now().floor())
            }))
        }),
        ("BitgetPrivateWebsocket", "markOrderAsOpenFromWs") => sync_member(rt, name, |rt, args| {
            let id = order_id_arg(&args)?;
            let clock = rt.clone();
            undefined_after(with_lifecycle(rt, |c| {
                c.mark_order_as_open_from_ws(&id, || clock.now().floor())
            }))
        }),
        _ => return Ok(None),
    };
    Ok(Some(member))
}
