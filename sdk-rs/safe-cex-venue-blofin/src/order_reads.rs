//! Active-order reconciliation and historical order/fill reads on configured REST.
use crate::{
    inline_tpsl::add_inline_tpsl_fallback_orders,
    order_history::{HistoryInput, map_blofin_historical_order},
    order_services::{ServiceError, ServiceHost, body, emit, field},
    orders::{OrdersInput, map_orders},
    symbols::{market_id_to_symbol, symbol_to_market_id},
    types::{ENDPOINTS, ORDER_SIDE},
    utility::UtilityInput,
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::{
    TableHit, js_number_string, js_string, js_table_lookup, js_truthy, math_max, math_min,
    multiply, parse_float, parse_int,
};
use safe_cex_store::Store;
use safe_cex_transport::{RestAdapter, RestRequest};
use safe_cex_types::{
    BothPositionSide, Execution, ExecutionPositionSide, ExecutionsOptions, HistoricalOrder, Order,
    OrderHistoryOptions, OrderSide, PositionSide, model_utils::normalize_execution_position_side,
};
use serde_json::{Map, Value, json};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
};
type OrdersFuture<'a, E> = Pin<Box<dyn Future<Output = Result<Vec<Order>, ServiceError<E>>> + 'a>>;
type Sleep<'a, E> =
    Box<dyn Fn(f64) -> Pin<Box<dyn Future<Output = Result<(), ServiceError<E>>> + 'a>> + 'a>;
/// Native snapshots replace JS aliases; clock remains infallible/non-mutating.
pub struct OrderReadHost<'a, E> {
    pub service: &'a ServiceHost<'a, E>,
    pub sleep: Sleep<'a, E>,
    pub now: &'a dyn Fn() -> f64,
}
fn endpoint(name: &str) -> &str {
    ENDPOINTS[name].as_str().expect("static endpoint")
}
async fn get<E>(
    xhr: &dyn RestAdapter,
    url: &str,
    params: Value,
) -> Result<UtilityInput, ServiceError<E>> {
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: url.into(),
            params: Some(params),
            ..Default::default()
        })
        .await;
    // A fulfilled JS await resumes later, even for an immediately-ready adapter.
    let mut yielded = false;
    poll_fn(|cx| {
        if !yielded {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    let response = response.map_err(|e| ServiceError::Rest(Box::new(e)))?;
    body(&response.body)
}
fn truth(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| js_truthy(Some(v)))
}
fn input<E>(text: impl Into<String>) -> ServiceError<E> {
    ServiceError::Input(text.into())
}
fn thrown<E>(value: Option<&Value>) -> Result<ServiceError<E>, ServiceError<E>> {
    Ok(input(match value {
        None => String::new(),
        Some(v) => js_string(Some(v)).map_err(ServiceError::Coercion)?,
    }))
}
fn fail<E>(host: &ServiceHost<'_, E>, value: &Value) -> Result<(), ServiceError<E>> {
    emit(host, Some(value))?;
    Err(thrown(Some(value))?)
}
fn log<E>(host: &ServiceHost<'_, E>, text: &str) -> Result<(), ServiceError<E>> {
    (host.log)(text, LogSeverity::Warning).map_err(ServiceError::Host)
}
pub(crate) fn error_value<E: std::fmt::Display>(
    error: &ServiceError<E>,
    include_message: bool,
    unknown: Option<&str>,
) -> Value {
    if let ServiceError::Rest(e) = error {
        if let Some(v) = e
            .response
            .as_ref()
            .and_then(|r| body::<E>(&r.body).ok())
            .and_then(|b| {
                truth(b.json().get("msg"))
                    .or_else(|| {
                        if include_message {
                            truth(b.json().get("message"))
                        } else {
                            None
                        }
                    })
                    .cloned()
            })
        {
            return v;
        }
        return Value::String(if e.message.is_empty() {
            unknown.unwrap_or("").into()
        } else {
            e.message.clone()
        });
    }
    let text = match error {
        ServiceError::Host(e) => e.to_string(),
        ServiceError::Input(e) => e.clone(),
        ServiceError::Ingress(e) => e.to_string(),
        ServiceError::Coercion(e) => e.message.into(),
        ServiceError::Store(e) => e.to_string(),
        ServiceError::Rest(_) => unreachable!(),
    };
    Value::String(if text.is_empty() {
        unknown.unwrap_or("").into()
    } else {
        text
    })
}
pub async fn fetch_paginated_order_rows<E>(
    url: &str,
    cursor_key: &str,
    family: &str,
    extra_params: &Map<String, Value>,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Vec<UtilityInput>, ServiceError<E>> {
    let mut orders: Vec<UtilityInput> = Vec::new();
    for page in 0..50 {
        let cursor = orders
            .last()
            .map(|r| field(r.json(), cursor_key))
            .transpose()?
            .flatten();
        let mut params = Map::new();
        if let Some(cursor) = cursor {
            params.insert("after".into(), cursor.clone());
        }
        params.insert("limit".into(), json!(100));
        params.extend(extra_params.clone());
        let data = get(xhr, url, Value::Object(params)).await?;
        let data = data.json();
        let values = field(data, "data")?;
        if !js_truthy(values) {
            let default = Value::String(format!("No {family} order data found"));
            let msg = truth(data.get("msg")).unwrap_or(&default);
            fail(host, msg)?;
        }
        let values = values.expect("truthy data");
        // Source's length===0 guard also accepts an ordinary object {length:0}.
        if values.get("length").and_then(Value::as_f64) == Some(0.0) {
            return Ok(orders);
        }
        let rows = values
            .as_array()
            .ok_or_else(|| input("Active order data is outside the array response profile"))?;
        if rows.is_empty() {
            return Ok(orders);
        }
        let next = field(rows.last().expect("nonempty"), cursor_key)?;
        let Some(next) = next.and_then(Value::as_str).filter(|v| !v.is_empty()) else {
            fail(
                host,
                &Value::String(format!(
                    "BloFin {family} order pagination returned an invalid {cursor_key}"
                )),
            )?;
            unreachable!()
        };
        if cursor.and_then(Value::as_str) == Some(next) {
            fail(
                host,
                &Value::String(format!(
                    "BloFin {family} order pagination cursor did not advance"
                )),
            )?;
        }
        orders.extend(rows.iter().cloned().map(UtilityInput::from_retained));
        if page == 49 {
            fail(
                host,
                &Value::String(format!(
                    "BloFin {family} order pagination exceeded 50 pages"
                )),
            )?;
        }
    }
    unreachable!("all page paths return")
}
async fn mapped<E>(
    key: &str,
    cursor: &str,
    family: &str,
    extra: Map<String, Value>,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &OrderReadHost<'_, E>,
) -> Result<Vec<Order>, ServiceError<E>> {
    let rows = fetch_paginated_order_rows(endpoint(key), cursor, family, &extra, xhr, host.service)
        .await?;
    map_orders(
        &OrdersInput::from_retained(Value::Array(
            rows.iter().map(|r| r.json().clone()).collect(),
        )),
        store,
        host.now,
    )
    .map_err(ServiceError::Coercion)
}
pub async fn fetch_normal_orders<E>(
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &OrderReadHost<'_, E>,
) -> Result<Vec<Order>, ServiceError<E>> {
    mapped(
        "UNFILLED_ORDERS",
        "orderId",
        "normal",
        Map::new(),
        store,
        xhr,
        host,
    )
    .await
}
pub async fn fetch_algo_orders<E>(
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &OrderReadHost<'_, E>,
) -> Result<Vec<Order>, ServiceError<E>> {
    mapped(
        "UNFILLED_ALGO_ORDERS",
        "tpslId",
        "TPSL",
        Map::new(),
        store,
        xhr,
        host,
    )
    .await
}
pub async fn fetch_trigger_orders<E>(
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &OrderReadHost<'_, E>,
) -> Result<Vec<Order>, ServiceError<E>> {
    mapped(
        "UNFILLED_TRIGGER_ORDERS",
        "algoId",
        "trigger",
        json!({"orderType":"trigger"})
            .as_object()
            .expect("object")
            .clone(),
        store,
        xhr,
        host,
    )
    .await
}
async fn three<E>(
    a: impl Future<Output = Result<Vec<Order>, ServiceError<E>>>,
    b: impl Future<Output = Result<Vec<Order>, ServiceError<E>>>,
    c: impl Future<Output = Result<Vec<Order>, ServiceError<E>>>,
) -> Result<Vec<Order>, ServiceError<E>> {
    let mut futures: Vec<OrdersFuture<'_, E>> = vec![Box::pin(a), Box::pin(b), Box::pin(c)];
    let mut out: [Option<Vec<Order>>; 3] = [None, None, None];
    poll_fn(|cx| {
        let mut error = None;
        for (future, slot) in futures.iter_mut().zip(&mut out) {
            if slot.is_none() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(Ok(v)) => *slot = Some(v),
                    Poll::Ready(Err(e)) => {
                        if error.is_none() {
                            error = Some(e)
                        }
                    }
                    Poll::Pending => (),
                }
            }
        }
        if let Some(e) = error {
            return Poll::Ready(Err(e));
        }
        if out.iter().all(Option::is_some) {
            Poll::Ready(Ok(out
                .iter_mut()
                .flat_map(|v| v.take().expect("complete"))
                .collect()))
        } else {
            Poll::Pending
        }
    })
    .await
}
pub async fn fetch_orders_with_retry<'a, E: std::fmt::Display>(
    retries: f64,
    store: &dyn Store,
    host: &OrderReadHost<'_, E>,
    fetch_families: &dyn Fn() -> [OrdersFuture<'a, E>; 3],
) -> Result<Vec<Order>, ServiceError<E>> {
    let mut last = None;
    let mut attempt = 1.0;
    while attempt <= retries {
        let outcome = async {
            let [normal, algo, trigger] = fetch_families();
            let orders = three(normal, algo, trigger).await?;
            let orders = add_inline_tpsl_fallback_orders(&orders, host.now)
                .map_err(ServiceError::Coercion)?;
            if orders.is_empty() && !store.orders().is_empty() {
                log(
                    host.service,
                    "Warning: fetchOrders returned empty but store had orders. Could be valid or API issue.",
                )?;
            }
            Ok(orders)
        }
        .await;
        match outcome {
            Ok(orders) => return Ok(orders),
            Err(e) => {
                last = Some(e);
                if attempt < retries {
                    log(
                        host.service,
                        &format!(
                            "fetchOrders attempt {} failed, retrying...",
                            js_number_string(attempt)
                        ),
                    )?;
                    (host.sleep)(1000.0 * attempt).await?;
                }
            }
        }
        attempt += 1.0;
    }
    // The retry catch reads error.message, not the optional response hint.
    let text = last
        .as_ref()
        .map(|e| match e {
            ServiceError::Rest(e) => e.message.clone(),
            _ => error_value(e, false, None)
                .as_str()
                .unwrap_or("")
                .to_owned(),
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "fetchOrders failed after retries".into());
    emit(host.service, Some(&Value::String(text)))?;
    Ok(store.orders().to_vec())
}
pub async fn fetch_orders<'a, E: std::fmt::Display>(
    _symbols: Option<&[String]>,
    store: &dyn Store,
    host: &OrderReadHost<'_, E>,
    fetch_families: &dyn Fn() -> [OrdersFuture<'a, E>; 3],
) -> Result<Vec<Order>, ServiceError<E>> {
    fetch_orders_with_retry(3.0, store, host, fetch_families).await
}
pub async fn fetch_historical_orders<E: std::fmt::Display>(
    opts: Option<&OrderHistoryOptions>,
    public_data_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Vec<HistoricalOrder>, ServiceError<E>> {
    if public_data_only {
        return Ok(Vec::new());
    }
    let symbol = opts
        .and_then(|o| o.symbol.as_deref())
        .filter(|s| !s.is_empty());
    let market = symbol.and_then(|s| store.markets_by_symbol().get(s).cloned());
    if symbol.is_some() && market.is_none() {
        return Ok(Vec::new());
    }
    if opts.is_some_and(|o| {
        o.from.is_some_and(|v| !v.is_finite())
            || o.to.is_some_and(|v| !v.is_finite())
            || o.limit.is_some_and(|v| v == f64::NEG_INFINITY)
    }) {
        return Err(input(
            "Nonfinite historical query parameter is outside native JSON",
        ));
    }
    let operation = async {
        let mut params = Map::new();
        if let Some(m) = market {
            params.insert("instId".into(), json!(m.id));
        }
        if let Some(v) = opts.and_then(|o| o.from) {
            params.insert("begin".into(), json!(v));
        }
        if let Some(v) = opts.and_then(|o| o.to) {
            params.insert("end".into(), json!(v));
        }
        let limit = opts
            .and_then(|o| o.limit)
            .filter(|v| *v != 0.0 && !v.is_nan())
            .unwrap_or(100.0);
        params.insert("limit".into(), json!(math_min(limit, 100.0)));
        let data = get::<E>(xhr, endpoint("ORDERS_HISTORY"), Value::Object(params)).await?;
        let mut orders = Vec::new();
        if let Some(rows) = data.json().get("data").and_then(Value::as_array) {
            for row in rows {
                if let Some(order) =
                    map_blofin_historical_order(&HistoryInput::from_retained(row.clone()), |id| {
                        store.markets_by_id().get(id).map(|m| m.symbol.clone())
                    })
                    .map_err(ServiceError::Coercion)?
                {
                    orders.push(order);
                }
            }
        }
        orders.sort_by(|a, b| {
            (b.created_time - a.created_time)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(orders)
    }
    .await;
    match operation {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host, Some(&error_value(&e, false, None)))?;
            Ok(Vec::new())
        }
    }
}
fn first<E>(row: &Value, keys: &[&str]) -> Result<Option<f64>, ServiceError<E>> {
    for key in keys {
        let value = field(row, key)?;
        if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
            continue;
        }
        let value = parse_float(&js_string(value).map_err(ServiceError::Coercion)?);
        if value.is_finite() {
            return Ok(Some(value));
        }
    }
    Ok(None)
}
fn millis(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite()).map(|v| {
        if v > 1_000_000_000_000.0 {
            v.floor()
        } else {
            (v * 1000.0).floor()
        }
    })
}
fn seconds<E>(value: Option<&Value>) -> Result<Option<f64>, ServiceError<E>> {
    let value = value.filter(|v| !v.is_null());
    let text = match value {
        Some(v) => js_string(Some(v)).map_err(ServiceError::Coercion)?,
        None => "0".into(),
    };
    let parsed = parse_int(&text, Some(10));
    Ok((parsed.is_finite() && parsed > 0.0).then(|| {
        if parsed > 1_000_000_000_000.0 {
            (parsed / 1000.0).floor()
        } else {
            parsed
        }
    }))
}
fn or_fields<'a, E>(row: &'a Value, keys: &[&str]) -> Result<Option<&'a Value>, ServiceError<E>> {
    for key in keys {
        let v = field(row, key)?;
        if js_truthy(v) {
            return Ok(v);
        }
    }
    Ok(None)
}
fn execution_rows<E>(
    rows: &[Value],
    market: &safe_cex_types::Market,
    contract_size: f64,
    opts: &ExecutionsOptions,
    store: &dyn Store,
) -> Result<Vec<Execution>, ServiceError<E>> {
    let mut out = Vec::new();
    for row in rows {
        if crate::utility::is_known_non_swap_instrument(&UtilityInput::from_retained(row.clone()))
            .map_err(ServiceError::Coercion)?
        {
            continue;
        }
        let side = js_string(
            or_fields(row, &["side", "actualSide"])?.or(Some(&Value::String(String::new()))),
        )
        .map_err(ServiceError::Coercion)?
        .to_lowercase();
        let table: Vec<_> = ORDER_SIDE
            .as_object()
            .expect("table")
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();
        let side = match js_table_lookup(&table, &side) {
            TableHit::Mapped(v) => {
                if v.as_str() == Some("buy") {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                }
            }
            TableHit::Missing => continue,
            TableHit::Inherited(_) => {
                return Err(input(
                    "Inherited fill side is outside the native OrderSide enum",
                ));
            }
        };
        let empty = Value::String(String::new());
        let raw_id = or_fields(row, &["instId", "symbol"])?.unwrap_or(&empty);
        let id = js_string(Some(raw_id)).map_err(ServiceError::Coercion)?;
        let symbol = if let Some(found) = store.markets_by_id().get(&id) {
            Some(found.symbol.clone())
        } else if raw_id.is_string() {
            market_id_to_symbol(&id, store)
        } else {
            // JS fallback keeps the original value; numeric 17 !== string "17".
            None
        };
        if symbol.as_deref() != Some(market.symbol.as_str()) {
            continue;
        }
        let price = first(
            row,
            &[
                "fillPrice",
                "fillPx",
                "avgFillPrice",
                "averagePrice",
                "avgPx",
                "execPrice",
                "executedPrice",
            ],
        )?;
        let qty = first(
            row,
            &[
                "fillSize",
                "filledSize",
                "actualSize",
                "fillSz",
                "tradeQty",
                "execQty",
                "executedQty",
                "accFillSz",
            ],
        )?
        .map(f64::abs);
        let (Some(price), Some(qty)) = (price, qty) else {
            continue;
        };
        if price <= 0.0 || qty <= 0.0 {
            continue;
        }
        let Some(timestamp) = seconds(or_fields(
            row,
            &[
                "fillTime",
                "execTime",
                "tradeTime",
                "uTime",
                "updateTime",
                "ts",
                "ctime",
            ],
        )?)?
        else {
            continue;
        };
        let order_id = js_string(
            or_fields(row, &["orderId", "ordId", "algoId", "tpslId"])?
                .or(Some(&Value::String(String::new()))),
        )
        .map_err(ServiceError::Coercion)?;
        if opts
            .order_id
            .as_ref()
            .is_some_and(|id| !id.is_empty() && !order_id.is_empty() && id != &order_id)
        {
            continue;
        }
        let qty = multiply(qty, contract_size);
        if !qty.is_finite() || qty <= 0.0 {
            continue;
        }
        let commission =
            first(row, &["fee", "fillFee", "execFee", "tradeFee", "actualFee"])?.map(f64::abs);
        let pnl = first(row, &["fillPnl"])?;
        let currency = or_fields(row, &["feeCurrency", "feeCcy", "feeAsset", "ccy"])?;
        let currency = currency
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    input("Fill commission asset is outside the native string model")
                })
            })
            .transpose()?;
        let position_side = truth(row.get("positionSide"))
            .map(|v| normalize_execution_position_side(Some(v)))
            .transpose()
            .map_err(ServiceError::Coercion)?
            .map(|v| match v {
                "long" => ExecutionPositionSide::Side(PositionSide::Long),
                "short" => ExecutionPositionSide::Side(PositionSide::Short),
                _ => ExecutionPositionSide::Both(BothPositionSide::Both),
            });
        out.push(Execution {
            id: truth(row.get("tradeId"))
                .map(|v| js_string(Some(v)))
                .transpose()
                .map_err(ServiceError::Coercion)?,
            timestamp,
            symbol: market.symbol.clone(),
            order_id: (!order_id.is_empty()).then_some(order_id),
            price,
            qty,
            side,
            commission,
            commission_asset: currency,
            position_side,
            realized_pnl: pnl,
            maker: None,
            venue_reported_revenue: None,
            venue_reported_revenue_asset: None,
        });
    }
    Ok(out)
}
async fn fill_rows<E>(
    key: &str,
    opts: &ExecutionsOptions,
    from: Option<f64>,
    to: Option<f64>,
    limit: f64,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
) -> Result<Vec<Value>, ServiceError<E>> {
    let mut params = Map::new();
    if let Some(id) = opts
        .symbol
        .as_deref()
        .and_then(|s| symbol_to_market_id(s, store))
    {
        params.insert("instId".into(), json!(id));
    }
    if key == "FILLS" {
        if let Some(id) = &opts.order_id {
            params.insert("orderId".into(), json!(id));
        }
    } else {
        params.insert("state".into(), json!("filled"));
    }
    if let Some(v) = from {
        params.insert("begin".into(), json!(v));
    }
    if let Some(v) = to {
        params.insert("end".into(), json!(v));
    }
    params.insert("limit".into(), json!(limit));
    let data = get(xhr, endpoint(key), Value::Object(params)).await?;
    let data = data.json();
    let code = data.get("code");
    if code.and_then(Value::as_str) != Some("0") && code.and_then(Value::as_f64) != Some(0.0) {
        let default = Value::String(format!(
            "Failed to fetch Blofin executions from {}",
            endpoint(key)
        ));
        return Err(thrown(truth(data.get("msg")).or(Some(&default)))?);
    }
    Ok(data
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}
fn unsupported<E: std::fmt::Display>(e: &ServiceError<E>) -> Result<bool, ServiceError<E>> {
    Ok(
        js_string(Some(&error_value(e, true, Some("Unknown Blofin error"))))
            .map_err(ServiceError::Coercion)?
            .to_lowercase()
            .contains("operation is not supported"),
    )
}
pub async fn fetch_historical_fills<E: std::fmt::Display>(
    opts: &ExecutionsOptions,
    public_data_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Vec<Execution>, ServiceError<E>> {
    if public_data_only {
        return Ok(Vec::new());
    }
    let Some(symbol) = opts.symbol.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    let market = store.markets_by_symbol().get(symbol).cloned();
    let Some(market) = market else {
        return Ok(Vec::new());
    };
    let contract_size = match market.contract_size.filter(|v| *v != 0.0 && !v.is_nan()) {
        Some(value) => value,
        None => {
            let default =
                safe_cex_types::retained_json::RetainedJsonInput::Text(vec![u16::from(b'1')]);
            safe_cex_num::js_retained_parse_float(
                market
                    .info
                    .as_ref()
                    .and_then(|v| v.get("contractValue"))
                    .filter(|v| v.is_truthy())
                    .or(Some(&default)),
            )
            .map_err(ServiceError::Coercion)?
        }
    };
    let limit = math_min(math_max(opts.limit.unwrap_or(100.0), 1.0), 100.0);
    if !limit.is_finite() {
        return Err(input("Nonfinite fill query limit is outside native JSON"));
    }
    let from = millis(opts.from);
    let to = millis(opts.to);
    if from.is_some_and(|v| !v.is_finite()) || to.is_some_and(|v| !v.is_finite()) {
        return Err(input("Nonfinite fill query time is outside native JSON"));
    }
    let operation = async {
        let primary = match fill_rows("FILLS", opts, from, to, limit, store, xhr).await {
            Ok(rows) => rows,
            Err(e) => {
                if !unsupported(&e)? {
                    return Err(e);
                }
                let text = js_string(Some(&error_value(&e, true, Some("Unknown Blofin error"))))
                    .map_err(ServiceError::Coercion)?;
                log(
                    host,
                    &format!("Blofin fills endpoint not supported for this account: {text}"),
                )?;
                Vec::new()
            }
        };
        let fallback = if primary.is_empty() {
            match fill_rows("ORDERS_HISTORY", opts, from, to, limit, store, xhr).await {
                Ok(rows) => rows,
                Err(e) => {
                    if !unsupported(&e)? {
                        return Err(e);
                    }
                    let text =
                        js_string(Some(&error_value(&e, true, Some("Unknown Blofin error"))))
                            .map_err(ServiceError::Coercion)?;
                    log(
                        host,
                        &format!("Blofin execution fallback not supported: {text}"),
                    )?;
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let mut result: Vec<(String, Execution)> = Vec::new();
        for e in execution_rows(&primary, &market, contract_size, opts, store)?
            .into_iter()
            .chain(execution_rows(
                &fallback,
                &market,
                contract_size,
                opts,
                store,
            )?)
        {
            if from.is_some_and(|v| v != 0.0 && e.timestamp * 1000.0 < v)
                || to.is_some_and(|v| v != 0.0 && e.timestamp * 1000.0 > v)
            {
                continue;
            }
            let key = if let Some(id) = &e.id {
                format!("id:{id}")
            } else {
                format!(
                    "{}:{}:{}:{}:{}",
                    e.order_id
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("no-order"),
                    js_number_string(e.timestamp),
                    js_number_string(e.price),
                    js_number_string(e.qty),
                    if e.side == OrderSide::Buy {
                        "buy"
                    } else {
                        "sell"
                    }
                )
            };
            if let Some(slot) = result.iter_mut().find(|(k, _)| k == &key) {
                slot.1 = e;
            } else {
                result.push((key, e));
            }
        }
        let mut result: Vec<_> = result.into_iter().map(|(_, e)| e).collect();
        result.sort_by(|a, b| {
            (b.timestamp - a.timestamp)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let count = if limit.is_nan() || limit <= 0.0 {
            0
        } else {
            limit.floor() as usize
        };
        result.truncate(count);
        Ok(result)
    }
    .await;
    match operation {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(
                host,
                Some(&error_value(&e, true, Some("Unknown Blofin error"))),
            )?;
            Ok(Vec::new())
        }
    }
}
