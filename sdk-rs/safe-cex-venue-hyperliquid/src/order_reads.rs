//! Primary open-order collection, surgical reconciliation and coalesced request wrappers.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone)]
pub enum OrderReadCallback<V> {
    ObserveIds {
        observed: V,
    },
    Id,
    IdPair,
    Void,
    /// findOrderByClientOrderId: `order.symbol === symbol && orderCarriesClientOrderId(order, id)`.
    ClientOrderMatch {
        symbol: V,
        client_order_id: V,
    },
}
/// Owned callbacks retain the shared runtime. Iterator operations use native records;
/// a throw from the loop body closes with original-throw precedence. The one-element
/// binding closes its own iterator after taking the leading element.
pub trait OrderReadHost: SpotAccountHost {
    fn order_read_callback(
        &mut self,
        c: OrderReadCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, v: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, v: &Self::Value);
    fn first_binding(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn typeof_value(&self, v: &Self::Value) -> &'static str;
    fn argument_spread(
        &mut self,
        out: &mut Vec<Self::Value>,
        v: &Self::Value,
    ) -> Result<(), Self::Error>;
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
fn object<H: SpotAccountHost>(
    h: &mut H,
    fields: &[(&str, H::Value)],
) -> Result<H::Value, H::Error> {
    let o = h.object()?;
    for (k, v) in fields {
        h.define_data(&o, k, v)?
    }
    Ok(o)
}
fn named<H: SpotAccountHost>(h: &mut H, import: &str, k: &str) -> Result<H::Value, H::Error> {
    let table = h.imported(import)?;
    h.get(&table, k)
}
fn log<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    severity: &str,
    text: impl FnOnce(&mut H) -> Result<JsString, H::Error>,
) -> Result<(), H::Error> {
    method(h, e, "log", |h| {
        let text = text(h)?;
        let text = h.literal(&text);
        let severity = named(h, "LogSeverity", severity)?;
        Ok(vec![text, severity])
    })?;
    Ok(())
}
fn format<H: SpotAccountHost>(
    h: &mut H,
    parts: &[(&str, Option<H::Value>)],
    suffix: &str,
) -> Result<JsString, H::Error> {
    let mut text = vec![];
    for (prefix, value) in parts {
        text.extend(prefix.encode_utf16());
        if let Some(v) = value {
            text.extend(h.template_string(v)?.0)
        }
    }
    text.extend(suffix.encode_utf16());
    Ok(JsString(text))
}
fn observed<H: OrderReadHost>(
    h: &mut H,
    set: &H::Value,
    orders: &H::Value,
) -> Result<(), H::Error> {
    if !h.is_array(orders)? {
        return Ok(());
    }
    let iterator = h.iterator(orders)?;
    while let Some(order) = h.next(&iterator)? {
        let step = (|| {
            if !h.truthy(&order) || h.typeof_value(&order) != "object" {
                return Ok(());
            }
            let id = h.get(&order, "oid")?;
            if !h.is_undefined(&id) && !h.strict_equal(&id, &h.null()) {
                method(h, set, "add", |h| {
                    let text = h.template_string(&id)?;
                    Ok(vec![h.literal(&text)])
                })?;
            }
            Ok(())
        })();
        if let Err(err) = step {
            h.close_after_throw(&iterator);
            return Err(err);
        }
    }
    Ok(())
}
pub fn run_order_read_callback<H: OrderReadHost>(
    h: &mut H,
    c: &OrderReadCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match c {
        OrderReadCallback::ObserveIds { observed: set } => {
            observed(h, set, &first)?;
            Ok(h.undefined())
        }
        OrderReadCallback::Id => h.get(&first, "id"),
        OrderReadCallback::IdPair => {
            let id = h.get(&first, "id")?;
            h.array(&[id, first])
        }
        OrderReadCallback::Void => Ok(h.undefined()),
        OrderReadCallback::ClientOrderMatch {
            symbol,
            client_order_id,
        } => {
            let own = h.get(&first, "symbol")?;
            if !h.strict_equal(&own, symbol) {
                return Ok(h.boolean(false));
            }
            let carries = h.imported("orderCarriesClientOrderId")?;
            let undefined = h.undefined();
            h.call(&carries, &undefined, &[first, client_order_id.clone()])
        }
    }
}
async fn post<H: OrderReadHost>(
    h: &mut H,
    e: &H::Value,
    user: &H::Value,
    dex: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let xhr = h.get(e, "xhr")?;
    let response = method(h, &xhr, "post", |h| {
        let endpoint = named(h, "ENDPOINTS", "INFO")?;
        let kind = named(h, "INFO_TYPES", "FRONTEND_OPEN_ORDERS")?;
        let body = object(h, &[("type", kind), ("user", user.clone())])?;
        if let Some(dex) = dex {
            h.define_data(&body, "dex", dex)?
        }
        let content = lit(h, "application/json");
        let headers = object(h, &[("Content-Type", content)])?;
        let config = object(h, &[("headers", headers)])?;
        Ok(vec![endpoint, body, config])
    })?;
    let response = h.await_value(response).await?;
    h.response_data(&response)
}
async fn internal_body<H: OrderReadHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let options = h.get(e, "options")?;
    let vault = h.get(&options, "vault")?;
    let user = if h.truthy(&vault) {
        vault
    } else {
        let options = h.get(e, "options")?;
        h.get(&options, "applicationId")?
    };
    let orders = h.array(&[])?;
    let constructor = h.global("Set")?;
    let observed_ids = h.construct(&constructor, &[])?;
    let record = h.order_read_callback(OrderReadCallback::ObserveIds {
        observed: observed_ids.clone(),
    })?;
    let result: Result<H::Value, H::Error> = async {
        let core = post(h, e, &user, None).await?;
        if h.is_array(&core)? {
            h.call(&record, &h.undefined(), std::slice::from_ref(&core))?;
            method(h, &orders, "push", |h| {
                let parsed = method(h, e, "parseOpenOrders", |_| Ok(vec![core]))?;
                let mut args = vec![];
                h.argument_spread(&mut args, &parsed)?;
                Ok(args)
            })?;
        }
        let constructor = h.global("Set")?;
        let fetched_dexes = h.construct(&constructor, &[])?;
        let dexes = h.get(e, "hip3DexNames")?;
        let iterator = h.iterator(&dexes)?;
        while let Some(dex) = h.next(&iterator)? {
            let step: Result<(), H::Error> = async {
                let fetch: Result<(), H::Error> = async {
                    log(h, e, "Debug", |h| {
                        format(
                            h,
                            &[("Fetching orders from HIP-3 dex: ", Some(dex.clone()))],
                            "",
                        )
                    })?;
                    let fetched = post(h, e, &user, Some(&dex)).await?;
                    log(h, e, "Debug", |h| {
                        let mut text = "HIP-3 dex ".encode_utf16().collect::<Vec<_>>();
                        text.extend(h.template_string(&dex)?.0);
                        text.extend(" returned ".encode_utf16());
                        let count = if h.is_array(&fetched)? {
                            h.get(&fetched, "length")?
                        } else {
                            h.number(0.)
                        };
                        text.extend(h.template_string(&count)?.0);
                        text.extend(" orders".encode_utf16());
                        Ok(JsString(text))
                    })?;
                    method(h, &fetched_dexes, "add", |h| {
                        Ok(vec![method(h, &dex, "toLowerCase", |_| Ok(vec![]))?])
                    })?;
                    if h.is_array(&fetched)? {
                        h.call(&record, &h.undefined(), std::slice::from_ref(&fetched))?;
                    }
                    if h.is_array(&fetched)? {
                        let len = h.get(&fetched, "length")?;
                        let positive = h.binary(">", &len, &h.number(0.))?;
                        if h.truthy(&positive) {
                            let parsed = method(h, e, "parseOpenOrders", |_| Ok(vec![fetched]))?;
                            log(h, e, "Info", |h| {
                                let count = h.get(&parsed, "length")?;
                                format(
                                    h,
                                    &[
                                        ("Successfully parsed ", Some(count)),
                                        (" orders from HIP-3 dex: ", Some(dex.clone())),
                                    ],
                                    "",
                                )
                            })?;
                            method(h, &orders, "push", |h| {
                                let mut args = vec![];
                                h.argument_spread(&mut args, &parsed)?;
                                Ok(args)
                            })?;
                        }
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = fetch {
                    let value = h.error_value(&error);
                    log(h, e, "Warning", |h| {
                        format(
                            h,
                            &[
                                ("Failed to fetch orders from HIP-3 dex ", Some(dex)),
                                (": ", Some(value)),
                            ],
                            "",
                        )
                    })?;
                }
                Ok(())
            }
            .await;
            if let Err(err) = step {
                h.close_after_throw(&iterator);
                return Err(err);
            }
        }
        let unique_fn = h.imported("uniqBy")?;
        let id_callback = h.order_read_callback(OrderReadCallback::Id)?;
        let unique = h.call(&unique_fn, &h.undefined(), &[orders, id_callback])?;
        let constructor = h.global("Set")?;
        let ids = method(h, &unique, "map", |h| {
            Ok(vec![h.order_read_callback(OrderReadCallback::Id)?])
        })?;
        let fetched_ids = h.construct(&constructor, &[ids])?;
        let store = h.get(e, "store")?;
        let current = h.get(&store, "orders")?;
        let current = h.spread_array(&current)?;
        let constructor = h.global("Map")?;
        let pairs = method(h, &current, "map", |h| {
            Ok(vec![h.order_read_callback(OrderReadCallback::IdPair)?])
        })?;
        let by_id = h.construct(&constructor, &[pairs])?;
        let date = h.global("Date")?;
        let now = method(h, &date, "now", |_| Ok(vec![]))?;
        let iterator = h.iterator(&unique)?;
        while let Some(new) = h.next(&iterator)? {
            let step = (|| {
                let cancelled = h.get(e, "_cancelledOrders")?;
                let cancel_time = method(h, &cancelled, "get", |h| Ok(vec![h.get(&new, "id")?]))?;
                if !h.is_undefined(&cancel_time) {
                    let elapsed = h.binary("-", &now, &cancel_time)?;
                    let window = h.get(e, "CANCEL_SUPPRESSION_TRUST_WINDOW_MS")?;
                    let recent = h.binary("<", &elapsed, &window)?;
                    if h.truthy(&recent) {
                        return Ok(());
                    }
                    method(h, e, "unsuppressCancelledOrder", |h| {
                        Ok(vec![h.get(&new, "id")?])
                    })?;
                    log(h, e, "Warning", |h| {
                        let id = h.get(&new, "id")?;
                        format(
                            h,
                            &[("Cancel suppression expired for order ", Some(id))],
                            " — REST still shows it as open, un-suppressing",
                        )
                    })?;
                }
                let existing = method(h, &by_id, "get", |h| Ok(vec![h.get(&new, "id")?]))?;
                if !h.truthy(&existing) {
                    let store = h.get(e, "store")?;
                    method(h, &store, "addOrder", |_| Ok(vec![new]))?;
                } else {
                    let mut changed = false;
                    for field in [
                        "status",
                        "price",
                        "amount",
                        "filled",
                        "type",
                        "timeInForce",
                        "clientOrderId",
                    ] {
                        let a = h.get(&existing, field)?;
                        let b = h.get(&new, field)?;
                        if !h.strict_equal(&a, &b) {
                            changed = true;
                            break;
                        }
                    }
                    if changed {
                        let store = h.get(e, "store")?;
                        method(h, &store, "updateOrder", |h| {
                            let id = h.get(&new, "id")?;
                            let selector = object(h, &[("id", id)])?;
                            Ok(vec![selector, new])
                        })?;
                    }
                }
                Ok(())
            })();
            if let Err(err) = step {
                h.close_after_throw(&iterator);
                return Err(err);
            }
        }
        let iterator = h.iterator(&current)?;
        while let Some(order) = h.next(&iterator)? {
            let step = (|| {
                let found = method(h, &fetched_ids, "has", |h| Ok(vec![h.get(&order, "id")?]))?;
                if h.truthy(&found) {
                    return Ok(());
                }
                let status = h.get(&order, "status")?;
                let creating = named(h, "OrderStatus", "Creating")?;
                if h.strict_equal(&status, &creating) {
                    return Ok(());
                }
                let status = h.get(&order, "status")?;
                let updating = named(h, "OrderStatus", "Updating")?;
                if h.strict_equal(&status, &updating) {
                    return Ok(());
                }
                let store = h.get(e, "store")?;
                let markets = h.get(&store, "marketsBySymbol")?;
                let symbol = h.get(&order, "symbol")?;
                let market = h.index(&markets, &symbol)?;
                let dex = if h.is_nullish(&market) {
                    h.undefined()
                } else {
                    let info = h.get(&market, "info")?;
                    if h.is_nullish(&info) {
                        h.undefined()
                    } else {
                        let dex = h.get(&info, "hip3Dex")?;
                        if h.is_nullish(&dex) {
                            h.undefined()
                        } else {
                            method(h, &dex, "toLowerCase", |_| Ok(vec![]))?
                        }
                    }
                };
                if h.truthy(&dex) {
                    let found = method(h, &fetched_dexes, "has", |_| Ok(vec![dex]))?;
                    if !h.truthy(&found) {
                        return Ok(());
                    }
                }
                let store = h.get(e, "store")?;
                method(h, &store, "removeOrder", |h| {
                    let id = h.get(&order, "id")?;
                    let selector = object(h, &[("id", id)])?;
                    Ok(vec![selector])
                })?;
                Ok(())
            })();
            if let Err(err) = step {
                h.close_after_throw(&iterator);
                return Err(err);
            }
        }
        let cancelled = h.get(e, "_cancelledOrders")?;
        let iterator = h.iterator(&cancelled)?;
        while let Some(entry) = h.next(&iterator)? {
            let step = (|| {
                let cancelled_id = h.first_binding(&entry)?;
                let found = method(h, &observed_ids, "has", |_| Ok(vec![cancelled_id.clone()]))?;
                if !h.truthy(&found) {
                    let cancelled = h.get(e, "_cancelledOrders")?;
                    method(h, &cancelled, "delete", |_| Ok(vec![cancelled_id]))?;
                }
                Ok(())
            })();
            if let Err(err) = step {
                h.close_after_throw(&iterator);
                return Err(err);
            }
        }
        Ok(unique)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(error) => {
            let error = h.error_value(&error);
            log(h, e, "Error", |h| {
                format(h, &[("Failed to fetch orders: ", Some(error))], "")
            })?;
            h.array(&[])
        }
    }
}
pub fn fetch_orders_internal<H: OrderReadHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    crate::helper_tasks::start(host, async move { internal_body(&mut h, &e).await })
}
async fn fetch_body<H: OrderReadHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let flight = h.get(e, "_fetchOrdersInFlight")?;
    if h.truthy(&flight) {
        return h.get(e, "_fetchOrdersInFlight");
    }
    let request = method(h, e, "fetchOrdersInternal", |_| Ok(vec![]))?;
    h.set(e, "_fetchOrdersInFlight", &request)?;
    let result = h.await_value(request.clone()).await;
    let flight = h.get(e, "_fetchOrdersInFlight")?;
    if h.strict_equal(&flight, &request) {
        h.set(e, "_fetchOrdersInFlight", &h.null())?;
    }
    result
}
pub fn fetch_orders<H: OrderReadHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
    _symbols: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    crate::helper_tasks::start(host, async move { fetch_body(&mut h, &e).await })
}
pub fn refresh_orders<H: OrderReadHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let request = method(h, e, "fetchOrders", |_| Ok(vec![]))?;
    method(h, &request, "then", |h| {
        Ok(vec![h.order_read_callback(OrderReadCallback::Void)?])
    })
}
/// ts: HyperliquidExchange.findOrderByClientOrderId. Asks `orderStatus` by cloid,
/// which also answers for filled/cancelled orders and never writes the store; a
/// miss is never definitive (a nonce up to two days old can still land).
async fn find_body<H: OrderReadHost>(
    h: &mut H,
    e: &H::Value,
    symbol: &H::Value,
    client_order_id: &H::Value,
) -> Result<H::Value, H::Error> {
    let not_found = object(
        h,
        &[
            ("found", h.boolean(false)),
            ("definitive", h.boolean(false)),
        ],
    )?;
    let store = h.get(e, "store")?;
    let orders = h.get(&store, "orders")?;
    let in_store = method(h, &orders, "find", |h| {
        Ok(vec![h.order_read_callback(
            OrderReadCallback::ClientOrderMatch {
                symbol: symbol.clone(),
                client_order_id: client_order_id.clone(),
            },
        )?])
    })?;
    if h.truthy(&in_store) {
        return object(h, &[("found", h.boolean(true)), ("order", in_store)]);
    }
    let options = h.get(e, "options")?;
    let vault = h.get(&options, "vault")?;
    let user = if h.truthy(&vault) {
        vault
    } else {
        let options = h.get(e, "options")?;
        h.get(&options, "applicationId")?
    };
    if !h.truthy(&user) {
        return Ok(not_found);
    }
    let pattern = h.imported("HYPERLIQUID_CLOID_PATTERN")?;
    let valid = method(h, &pattern, "test", |_| Ok(vec![client_order_id.clone()]))?;
    if !h.truthy(&valid) {
        return Ok(not_found);
    }
    let xhr = h.get(e, "infoXhr")?;
    let response = method(h, &xhr, "post", |h| {
        let endpoint = named(h, "ENDPOINTS", "INFO")?;
        let kind = named(h, "INFO_TYPES", "ORDER_STATUS")?;
        let body = object(
            h,
            &[
                ("type", kind),
                ("user", user.clone()),
                ("oid", client_order_id.clone()),
            ],
        )?;
        let content = lit(h, "application/json");
        let headers = object(h, &[("Content-Type", content)])?;
        let config = object(h, &[("headers", headers)])?;
        Ok(vec![endpoint, body, config])
    })?;
    let response = h.await_value(response).await?;
    let data = h.get(&response, "data")?;
    // data?.status !== 'order' || !data.order?.order
    let status = if h.is_nullish(&data) {
        h.undefined()
    } else {
        h.get(&data, "status")?
    };
    if !h.strict_equal(&status, &lit(h, "order")) {
        return Ok(not_found);
    }
    let order = h.get(&data, "order")?;
    let row = if h.is_nullish(&order) {
        h.undefined()
    } else {
        h.get(&order, "order")?
    };
    if !h.truthy(&row) {
        return Ok(not_found);
    }
    let order = h.get(&data, "order")?;
    let venue_status = h.get(&order, "status")?;
    let status = if h.strict_equal(&venue_status, &lit(h, "open"))
        || h.strict_equal(&venue_status, &lit(h, "triggered"))
    {
        named(h, "OrderStatus", "Open")?
    } else if h.strict_equal(&venue_status, &lit(h, "filled")) {
        named(h, "OrderStatus", "Closed")?
    } else {
        named(h, "OrderStatus", "Canceled")?
    };
    let mapped = method(h, e, "mapOpenOrderRow", |h| {
        let order = h.get(&data, "order")?;
        Ok(vec![h.get(&order, "order")?, status])
    })?;
    if h.truthy(&mapped) {
        let own = h.get(&mapped, "symbol")?;
        if h.strict_equal(&own, symbol) {
            return object(h, &[("found", h.boolean(true)), ("order", mapped)]);
        }
    }
    Ok(not_found)
}
pub fn find_order_by_client_order_id<H: OrderReadHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    e: &H::Value,
    symbol: &H::Value,
    client_order_id: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let e = e.clone();
    let symbol = symbol.clone();
    let client_order_id = client_order_id.clone();
    crate::helper_tasks::start(host, async move {
        find_body(&mut h, &e, &symbol, &client_order_id).await
    })
}
