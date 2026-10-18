//! Order REST orchestration; construction and callback ownership belong to the host.
use crate::{
    modes::{AccountModes, apply_confirmed_margin_type},
    order_formatters::OrderPayload,
    order_services::{ServiceError, ServiceHost, body, code_ok, emit, request},
    types::USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY,
    utility::{UtilityInput, is_already_gone_cancel_row},
};
use safe_cex_num::{
    js_number, js_number_string, js_string, js_trim, js_truthy, math_max, math_min, parse_float,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::RestAdapter;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    FetchedLeverage, MarginType, Order, OrderStatus, OrderTimeInForce, OrderType, Position,
    PositionMode, PreparedOrderOpts,
};
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
};

type HostFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, ServiceError<E>>> + 'a>>;
type NormalFormatter<'a, E> =
    Box<dyn Fn(PreparedOrderOpts) -> HostFuture<'a, Vec<OrderPayload>, E> + 'a>;
type AlgoFormatter<'a, E> = Box<dyn Fn(PreparedOrderOpts) -> HostFuture<'a, OrderPayload, E> + 'a>;
type TriggerFormatter<'a, E> =
    Box<dyn Fn(PreparedOrderOpts, Option<Input>) -> HostFuture<'a, OrderPayload, E> + 'a>;
type Placement<'a, E> =
    Box<dyn Fn(PreparedOrderOpts) -> HostFuture<'a, Vec<Option<Value>>, E> + 'a>;
type MarginReader<'a, E> = Box<dyn Fn(Option<String>, bool) -> HostFuture<'a, MarginType, E> + 'a>;
type ModeReader<'a, E> = Box<dyn Fn() -> HostFuture<'a, PositionMode, E> + 'a>;
type PositionsReader<'a, E> = Box<dyn Fn(Vec<String>) -> HostFuture<'a, Vec<Position>, E> + 'a>;
type ClientGuard<'a, E> = Box<dyn Fn(&PreparedOrderOpts) -> Result<(), ServiceError<E>> + 'a>;
type BatchGuard<'a, E> = Box<dyn Fn(&[PreparedOrderOpts]) -> Result<(), ServiceError<E>> + 'a>;
/// Supply the already ported formatters/mode readers, caller-ID policy and scheduler.
/// Arguments and results are owned snapshots, not executable objects or JS aliases.
pub struct WriteHost<'a, E> {
    pub service: &'a ServiceHost<'a, E>,
    pub format_normal_order: NormalFormatter<'a, E>,
    pub format_algo_order: AlgoFormatter<'a, E>,
    pub format_trigger_order: TriggerFormatter<'a, E>,
    pub place_order: Placement<'a, E>,
    pub get_margin_type: MarginReader<'a, E>,
    pub get_position_mode: ModeReader<'a, E>,
    pub fetch_positions: PositionsReader<'a, E>,
    pub sleep: Box<dyn Fn(u64) -> HostFuture<'a, (), E> + 'a>,
    pub resolve_caller_client_order_id: ClientGuard<'a, E>,
    pub assert_caller_client_order_ids: BatchGuard<'a, E>,
}
fn algo(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
fn truth_number(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn truth(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| js_truthy(Some(v)))
}
fn message<'a>(row: Option<&'a Value>, data: &'a Value) -> Option<&'a Value> {
    truth(row.and_then(|r| r.get("msg"))).or_else(|| data.get("msg"))
}
fn js_err<E>(value: Option<&Value>) -> Result<ServiceError<E>, ServiceError<E>> {
    // Error(undefined) has an empty message, unlike String(undefined).
    Ok(ServiceError::Input(match value {
        None => String::new(),
        Some(v) => js_string(Some(v)).map_err(ServiceError::Coercion)?,
    }))
}
fn reject<E>(host: &ServiceHost<'_, E>, value: Option<&Value>) -> Result<(), ServiceError<E>> {
    emit(host, value)?;
    Err(js_err(value)?)
}
fn rows<E>(value: Option<&Value>) -> Result<&[Value], ServiceError<E>> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| ServiceError::Input("data.reduce is not a function".into()))
}
fn response_row(data: &Value) -> Option<&Value> {
    data.get("data").and_then(|v| v.get(0))
}
fn successful_message<E>(msg: Option<&Value>) -> Result<bool, ServiceError<E>> {
    if msg.is_none_or(Value::is_null) {
        return Ok(true);
    }
    let text = js_string(msg).map_err(ServiceError::Coercion)?;
    Ok(matches!(
        js_trim(&text).to_lowercase().as_str(),
        "success" | "order placed"
    ))
}
async fn parallel<'a, T, E>(
    mut futures: Vec<HostFuture<'a, T, E>>,
) -> Result<Vec<T>, ServiceError<E>> {
    let mut outputs: Vec<Option<T>> = (0..futures.len()).map(|_| None).collect();
    poll_fn(|cx| {
        let mut pending = false;
        let mut error = None;
        for (future, output) in futures.iter_mut().zip(&mut outputs) {
            if output.is_some() {
                continue;
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(Ok(value)) => *output = Some(value),
                Poll::Ready(Err(e)) => {
                    if error.is_none() {
                        error = Some(e);
                    }
                }
                Poll::Pending => pending = true,
            }
        }
        if let Some(error) = error {
            return Poll::Ready(Err(error));
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(outputs
                .iter_mut()
                .map(|v| v.take().expect("complete output"))
                .collect()))
        }
    })
    .await
}
async fn pair<A, B, E>(
    a: impl Future<Output = Result<A, ServiceError<E>>>,
    b: impl Future<Output = Result<B, ServiceError<E>>>,
) -> Result<(A, B), ServiceError<E>> {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let mut av = None;
    let mut bv = None;
    poll_fn(|cx| {
        let mut error = None;
        if av.is_none() {
            match a.as_mut().poll(cx) {
                Poll::Ready(Ok(v)) => av = Some(v),
                Poll::Ready(Err(e)) => error = Some(e),
                Poll::Pending => (),
            }
        }
        if bv.is_none() {
            match b.as_mut().poll(cx) {
                Poll::Ready(Ok(v)) => bv = Some(v),
                Poll::Ready(Err(e)) => {
                    if error.is_none() {
                        error = Some(e);
                    }
                }
                Poll::Pending => (),
            }
        }
        if let Some(error) = error {
            return Poll::Ready(Err(error));
        }
        if av.is_some() && bv.is_some() {
            Poll::Ready(Ok((
                av.take().expect("ready a"),
                bv.take().expect("ready b"),
            )))
        } else {
            Poll::Pending
        }
    })
    .await
}
/// Normal order IDs are strings; numeric IDs are dropped and object IDs refuse,
/// matching the source's final filter after all submissions have completed.
pub async fn place_order_batch<E>(
    payloads: &[OrderPayload],
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Vec<String>, ServiceError<E>> {
    let single = |p: &&OrderPayload| {
        USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY && js_truthy(p.fields.get("reduceOnly"))
    };
    let eligible: Vec<_> = payloads.iter().filter(|p| !single(p)).collect();
    let noneligible: Vec<_> = payloads.iter().filter(single).collect();
    let mut ids = Vec::new();
    for chunk in eligible.chunks(20) {
        let data = request(
            xhr,
            "POST",
            "PLACE_ORDERS_BATCH",
            None,
            Some(Value::Array(
                chunk
                    .iter()
                    .map(|p| Value::Object(p.fields.clone()))
                    .collect(),
            )),
        )
        .await?;
        let data = data.json();
        if !code_ok(data)? {
            reject(host, message(response_row(data), data))?;
        }
        for row in rows(data.get("data"))? {
            if !successful_message(crate::order_services::field(row, "msg")?)? {
                reject(host, crate::order_services::field(row, "msg")?)?;
            }
            if let Some(id) = truth(row.get("orderId")) {
                ids.push(id.clone());
            }
        }
    }
    for payload in noneligible {
        let data = request(
            xhr,
            "POST",
            "PLACE_ORDERS",
            None,
            Some(Value::Object(payload.fields.clone())),
        )
        .await?;
        for row in rows(data.json().get("data"))? {
            let Some(id) = truth(row.get("orderId")) else {
                reject(host, row.get("msg"))?;
                unreachable!()
            };
            if !successful_message(row.get("msg"))? {
                reject(host, row.get("msg"))?;
            }
            ids.push(id.clone());
        }
    }
    let mut results = Vec::new();
    for id in ids {
        if id.is_object() || id.is_array() {
            return Err(js_err(id.get("msg"))?);
        }
        if let Value::String(id) = id {
            results.push(id);
        }
    }
    Ok(results)
}
pub async fn place_algo_order_batch<E>(
    payloads: &[OrderPayload],
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Vec<Option<Value>>, ServiceError<E>> {
    let mut ids = Vec::new();
    for payload in payloads {
        let data = request(
            xhr,
            "POST",
            "PLACE_TPSL_ORDER",
            None,
            Some(Value::Object(payload.fields.clone())),
        )
        .await?;
        let data = data.json();
        if !code_ok(data)? {
            let special=Value::String("BloFin Multi-Position mode is not supported in Tealstreet yet. Turn off Multi-Position mode on BloFin, then retry from Tealstreet.".into());
            let msg = if data.get("code").and_then(Value::as_str) == Some("102143") {
                Some(&special)
            } else {
                message(response_row(data), data)
            };
            reject(host, msg)?;
        }
        ids.push(data.get("data").and_then(|r| r.get("tpslId")).cloned());
    }
    Ok(ids)
}
fn size_zero<E>(payloads: &[OrderPayload]) -> Result<bool, ServiceError<E>> {
    for payload in payloads {
        if parse_float(&js_string(payload.fields.get("size")).map_err(ServiceError::Coercion)?)
            == 0.0
        {
            return Ok(true);
        }
    }
    Ok(false)
}
pub async fn place_order<E>(
    opts: &PreparedOrderOpts,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<Vec<Option<Value>>, ServiceError<E>> {
    (host.resolve_caller_client_order_id)(opts)?;
    if opts.r#type == OrderType::Conditional {
        let payload = (host.format_trigger_order)(opts.clone(), None).await?;
        let id = crate::order_services::place_trigger_order(&payload, xhr, host.service).await?;
        return Ok(vec![Some(Value::String(id))]);
    }
    if opts.r#type.is_some_and(algo) {
        let payload = (host.format_algo_order)(opts.clone()).await?;
        return place_algo_order_batch(&[payload], xhr, host.service).await;
    }
    let payloads = (host.format_normal_order)(opts.clone()).await?;
    if size_zero(&payloads)? {
        reject(
            host.service,
            Some(&Value::String("Order amount is too small".into())),
        )?;
    }
    Ok(place_order_batch(&payloads, xhr, host.service)
        .await?
        .into_iter()
        .map(|id| Some(Value::String(id)))
        .collect())
}
pub async fn place_orders<E>(
    opts: &[PreparedOrderOpts],
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<Vec<Option<Value>>, ServiceError<E>> {
    (host.assert_caller_client_order_ids)(opts)?;
    let (triggers, rest): (Vec<_>, Vec<_>) = opts
        .iter()
        .partition(|o| o.r#type == OrderType::Conditional);
    let (algos, normals): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|o| o.r#type.is_some_and(algo));
    pair(
        (host.get_position_mode)(),
        (host.get_margin_type)(None, false),
    )
    .await?;
    let payloads = parallel(
        normals
            .into_iter()
            .map(|o| (host.format_normal_order)(o.clone()))
            .collect(),
    )
    .await?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if size_zero(&payloads)? {
        emit(
            host.service,
            Some(&Value::String("Total order amount is too small".into())),
        )?;
        return Ok(Vec::new());
    }
    let algo_payloads = parallel(
        algos
            .into_iter()
            .map(|o| (host.format_algo_order)(o.clone()))
            .collect(),
    )
    .await?;
    let mut trigger_ids = Vec::new();
    for opts in triggers {
        let payload = (host.format_trigger_order)(opts.clone(), None).await?;
        trigger_ids.push(Some(Value::String(
            crate::order_services::place_trigger_order(&payload, xhr, host.service).await?,
        )));
    }
    let mut ids = place_order_batch(&payloads, xhr, host.service)
        .await?
        .into_iter()
        .map(|id| Some(Value::String(id)))
        .collect::<Vec<_>>();
    ids.extend(place_algo_order_batch(&algo_payloads, xhr, host.service).await?);
    ids.extend(trigger_ids);
    Ok(ids)
}
async fn cancel_request<E: std::fmt::Display>(
    endpoint: &str,
    payload: Value,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<UtilityInput, ServiceError<E>> {
    match request(xhr, "POST", endpoint, None, Some(payload)).await {
        Ok(data) => Ok(data),
        Err(error) => {
            // Only transport rejection is inside the source catch. Decoding is an explicit native boundary.
            if !matches!(error, ServiceError::Rest(_)) {
                return Err(error);
            }
            let ServiceError::Rest(failure) = error else {
                unreachable!()
            };
            let hint = failure
                .response
                .as_ref()
                .and_then(|r| body::<E>(&r.body).ok())
                .and_then(|v| v.json().get("msg").filter(|v| js_truthy(Some(v))).cloned());
            let fallback = Value::String(failure.message.clone());
            reject(host, hint.as_ref().or(Some(&fallback)))?;
            unreachable!()
        }
    }
}
fn gone<E>(row: &Value) -> Result<bool, ServiceError<E>> {
    is_already_gone_cancel_row(Some(&UtilityInput::from_retained(row.clone())))
        .map_err(ServiceError::Coercion)
}
async fn cancel_batches<E: std::fmt::Display>(
    orders: &[Order],
    is_algo: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    let mut payloads = Vec::new();
    for order in orders
        .iter()
        .filter(|o| o.r#type.is_some_and(algo) == is_algo)
    {
        let market = store.markets_by_symbol()?.get(&order.symbol).cloned();
        let Some(market) = market else { continue };
        payloads.push(if is_algo {
            json!({"instId":market.id,"tpslId":order.id.split("__").next().unwrap_or("")})
        } else {
            json!({"instId":market.id,"orderId":order.id})
        });
    }
    for chunk in payloads.chunks(if is_algo { 10 } else { 20 }) {
        let data = cancel_request(
            if is_algo {
                "CANCEL_ALGO_ORDER"
            } else {
                "CANCEL_ORDERS"
            },
            Value::Array(chunk.to_vec()),
            xhr,
            host.service,
        )
        .await?;
        let data = data.json();
        if !code_ok(data)? {
            let raw = data.get("data").filter(|v| !v.is_null());
            let empty = Value::Array(Vec::new());
            let rows = rows(raw.or(Some(&empty)))?;
            let mut failures = Vec::new();
            for row in rows {
                let id = if is_algo {
                    truth(crate::order_services::field(row, "tpslId")?)
                        .or_else(|| row.get("algoId"))
                } else {
                    row.get("orderId")
                };
                if gone(row)? && js_truthy(id) {
                    if is_algo {
                        // Strict comparison: a numeric wire ID never equals a native string ID.
                        let ids: Vec<_> = store
                            .orders()?
                            .iter()
                            .filter(|o| {
                                id.and_then(Value::as_str)
                                    == Some(o.id.split("__").next().unwrap_or(""))
                            })
                            .map(|o| o.id.clone())
                            .collect();
                        if !ids.is_empty() {
                            store
                                .remove_orders(&ids.iter().map(String::as_str).collect::<Vec<_>>())
                                .map_err(ServiceError::Store)?;
                        }
                    } else {
                        // DefaultStore.removeOrders builds an object-key lookup, unlike
                        // the protective path's strict root-ID comparison above.
                        let key = js_string(id).map_err(ServiceError::Coercion)?;
                        store.remove_orders(&[&key]).map_err(ServiceError::Store)?;
                    }
                } else {
                    failures.push(row);
                }
            }
            if rows.is_empty() || !failures.is_empty() {
                let msg = truth(failures.first().and_then(|r| r.get("msg")))
                    .or_else(|| truth(rows.first().and_then(|r| r.get("msg"))))
                    .or_else(|| data.get("msg"));
                reject(host.service, msg)?;
            }
        }
        if is_algo {
            (host.sleep)(1000).await?;
        }
    }
    Ok(())
}
pub async fn cancel_normal_orders<E: std::fmt::Display>(
    orders: &[Order],
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    cancel_batches(orders, false, store, xhr, host).await
}
pub async fn cancel_algo_orders<E: std::fmt::Display>(
    orders: &[Order],
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    cancel_batches(orders, true, store, xhr, host).await
}
pub async fn cancel_trigger_orders<E: std::fmt::Display>(
    orders: &[Order],
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    // Validate the entire set before the first write.
    let mut payloads = Vec::new();
    for order in orders {
        let market = store.markets_by_symbol()?.get(&order.symbol).cloned();
        let raw = order.info.as_ref();
        let id = raw
            .and_then(|v| v.get("algoId"))
            .filter(|v| v.is_truthy())
            .cloned()
            .unwrap_or_else(|| Input::Text(order.id.encode_utf16().collect()));
        let id = safe_cex_num::js_retained_string(Some(&id)).map_err(ServiceError::Coercion)?;
        if market.as_ref().is_none_or(|m| {
            !raw.and_then(|v| v.get("instId"))
                .is_some_and(|v| v.text_equals(&m.id))
        }) || !raw
            .and_then(|v| v.get("orderType"))
            .is_some_and(|v| v.text_equals("trigger"))
            || id != order.id
        {
            return Err(ServiceError::Input(format!(
                "Cannot safely cancel BloFin trigger order {}: authoritative provenance is missing",
                order.id
            )));
        }
        payloads.push(json!({"instId":market.expect("validated market").id,"algoId":id}));
    }
    for payload in payloads {
        let data =
            cancel_request("CANCEL_TRIGGER_ORDER", payload.clone(), xhr, host.service).await?;
        let data = data.json();
        let row = data.get("data");
        if row.map(gone::<E>).transpose()?.unwrap_or(false)
            && row.and_then(|r| r.get("algoId")) == payload.get("algoId")
        {
            store
                .remove_orders(&[payload["algoId"].as_str().expect("string id")])
                .map_err(ServiceError::Store)?;
        } else if data.get("code").and_then(Value::as_str) != Some("0")
            || row
                .and_then(|r| r.get("code"))
                .is_some_and(|c| c.as_str() != Some("0"))
            || row.and_then(|r| r.get("algoId")) != payload.get("algoId")
        {
            let default =
                Value::String("BloFin trigger cancellation returned an invalid response".into());
            let msg = truth(row.and_then(|r| r.get("msg")))
                .or_else(|| truth(data.get("msg")))
                .or(Some(&default));
            reject(host.service, msg)?;
        }
        (host.sleep)(350).await?;
    }
    Ok(())
}
fn from_order(order: &Order) -> PreparedOrderOpts {
    PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: order.r#type.into_property(),
        side: order.side.into_property(),
        amount: order
            .extra
            .as_ref()
            .and_then(|e| e.remaining_native)
            .unwrap_or(order.amount),
        price: order.price.into_value(),
        trigger_price: None,
        stop_loss: (order.stop_loss).into_value(),
        take_profit: (order.take_profit).into_value(),
        reduce_only: order.reduce_only.into_property(),
        time_in_force: (order.time_in_force).into_value(),
        trigger_type: (order.trigger_type).into_value(),
        tp_trigger_type: (order.tp_trigger_type).into_value(),
        sl_trigger_type: (order.sl_trigger_type).into_value(),
        client_order_id: None,
        max_slippage_pct: None,
        close: (order.close).into_value(),
        implied_volatility: None,
    }
}
pub async fn cancel_orders<E: std::fmt::Display>(
    orders: &[Order],
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    let preserve = if let [order] = orders
        && order.id.contains("__")
        && !truth_number(order.amount)
    {
        let root = order.id.split("__").next().unwrap_or("");
        let related: Vec<_> = store
            .orders()?
            .iter()
            .filter(|o| o.id.starts_with(root))
            .cloned()
            .collect();
        if related.len() > 1
            && (order.id.ends_with("__take_profit") || order.id.ends_with("__stop_loss"))
        {
            related.into_iter().find(|o| {
                o.id.ends_with(if order.id.ends_with("__take_profit") {
                    "__stop_loss"
                } else {
                    "__take_profit"
                }) && !truth_number(o.amount)
            })
        } else {
            None
        }
    } else {
        None
    };
    let mut normalized = Vec::new();
    for order in orders {
        let order = if order.id.contains("__") {
            let id = order.id.split("__").next().unwrap_or("");
            store
                .orders()?
                .iter()
                .find(|o| o.id == id)
                .cloned()
                .unwrap_or_else(|| {
                    let mut order = order.clone();
                    order.id = id.into();
                    order
                })
        } else {
            order.clone()
        };
        if !normalized.iter().any(|o: &Order| o.id == order.id) {
            normalized.push(order);
        }
    }
    let (triggers, rest): (Vec<_>, Vec<_>) = normalized
        .into_iter()
        .partition(|o| o.r#type == OrderType::Conditional);
    let (algos, normals): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|o| o.r#type.is_some_and(algo));
    if !normals.is_empty() {
        cancel_normal_orders(&normals, store, xhr, host).await?;
    }
    if !algos.is_empty() {
        cancel_algo_orders(&algos, store, xhr, host).await?;
    }
    if !triggers.is_empty() {
        cancel_trigger_orders(&triggers, store, xhr, host).await?;
    }
    if let Some(order) = preserve {
        let mut replacement = from_order(&order);
        replacement.amount = 0.0;
        replacement.reduce_only = true.into();
        replacement.time_in_force = None;
        replacement.trigger_type = None;
        replacement.stop_loss = None;
        replacement.take_profit = None;
        replacement.close = None;
        // Preserve only the source's type-specific trigger axis.
        if order.r#type == OrderType::StopLoss {
            replacement.tp_trigger_type = None;
        } else if order.r#type == OrderType::TakeProfit {
            replacement.sl_trigger_type = None;
        } else {
            replacement.tp_trigger_type = None;
            replacement.sl_trigger_type = None;
        }
        (host.sleep)(500).await?;
        (host.place_order)(replacement).await?;
    }
    Ok(())
}
pub async fn change_margin_type<E>(
    isolated: bool,
    symbol: Option<&str>,
    modes: &AccountModes,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    if store.positions()?.iter().any(|p| p.contracts > 0.0)
        || store
            .orders()?
            .iter()
            .any(|o| o.status == OrderStatus::Open)
    {
        emit(host.service,Some(&Value::String("Changing margin type with open positions / orders is allowed but can cause unexpected behavior. Symbols with open positions will remain synced to previous mode until position is closed.".into())))?;
    }
    let data = request(
        xhr,
        "POST",
        "SET_MARGIN_MODE",
        None,
        Some(json!({"marginMode":if isolated{"isolated"}else{"cross"}})),
    )
    .await?;
    let data = data.json();
    if !code_ok(data)? {
        reject(host.service, data.get("msg"))?;
    }
    let confirmed = match data
        .get("data")
        .and_then(|data| data.get("marginMode"))
        .and_then(Value::as_str)
    {
        Some("isolated") => MarginType::Isolated,
        Some("cross") => MarginType::Cross,
        _ if isolated => MarginType::Isolated,
        _ => MarginType::Cross,
    };
    apply_confirmed_margin_type(modes, store, confirmed).map_err(ServiceError::Store)?;
    (host.get_margin_type)(None, true).await?;
    if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
        let positions = (host.fetch_positions)(vec![symbol.into()]).await?;
        store
            .add_or_update_positions(positions)
            .map_err(ServiceError::Store)?;
    }
    Ok(())
}
pub async fn set_leverage<E>(
    symbol: &str,
    input_leverage: f64,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    let market = store
        .markets_by_symbol()?
        .get(symbol)
        .cloned()
        .ok_or_else(|| ServiceError::Input(format!("Market {symbol} not found on Blofin")))?;
    let leverage = math_min(
        math_max(input_leverage, market.limits.leverage.min),
        market.limits.leverage.max,
    );
    let existing: Vec<_> = store
        .positions()?
        .iter()
        .filter(|p| p.symbol == symbol)
        .cloned()
        .collect();
    if !existing.is_empty() && existing.iter().all(|p| p.leverage == leverage) {
        return Ok(());
    }
    if existing.is_empty()
        && store.fetched_leverage()?.get(symbol).is_some_and(|l| {
            l.is_fetching != Some(true) && l.buy_leverage == leverage && l.sell_leverage == leverage
        })
    {
        return Ok(());
    }
    let (margin, mode) = pair(
        (host.get_margin_type)(None, false),
        (host.get_position_mode)(),
    )
    .await?;
    let base = json!({"instId":market.id,"leverage":js_number_string(leverage),"marginMode":if margin==MarginType::Isolated{"isolated"}else{"cross"}});
    let datas = if margin == MarginType::Isolated && mode == PositionMode::Hedged {
        let mut long = base.clone();
        long["positionSide"] = json!("long");
        let mut short = base;
        short["positionSide"] = json!("short");
        let (long, short) = pair(
            request(xhr, "POST", "SET_LEVERAGE", None, Some(long)),
            request(xhr, "POST", "SET_LEVERAGE", None, Some(short)),
        )
        .await?;
        vec![long, short]
    } else {
        vec![request(xhr, "POST", "SET_LEVERAGE", None, Some(base)).await?]
    };
    for data in datas {
        if !code_ok(data.json())? {
            reject(host.service, data.json().get("msg"))?;
        }
    }
    if !existing.is_empty() {
        let updates = existing
            .into_iter()
            .map(|mut p| {
                p.leverage = leverage;
                p
            })
            .collect();
        store
            .add_or_update_positions(updates)
            .map_err(ServiceError::Store)?;
    }
    let mut fetched = store.fetched_leverage()?.clone();
    fetched.insert(
        symbol.into(),
        FetchedLeverage {
            buy_leverage: leverage,
            sell_leverage: leverage,
            margin_type: Some(margin),
            position_mode: Some(mode),
            is_fetching: None,
        },
    );
    store
        .update(
            StoreUpdate {
                fetched_leverage: Some(fetched),
                ..Default::default()
            },
            false,
        )
        .map_err(ServiceError::Store)?;
    Ok(())
}
/// Update owns only the fields that change; source-spread unknown properties are
/// excluded by the shared typed Order/PreparedOrderOpts boundary.
pub async fn update_order<E: std::fmt::Display>(
    order: &Order,
    update: &UtilityInput,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &WriteHost<'_, E>,
) -> Result<Vec<Option<Value>>, ServiceError<E>> {
    let update = update.json();
    let object = update
        .as_object()
        .ok_or_else(|| ServiceError::Input("Update must be an own-property object".into()))?;
    for key in ["price", "amount"] {
        if object.get(key).is_some_and(|v| !v.is_number()) {
            return Err(ServiceError::Input(format!(
                "Update {key} must be a native number"
            )));
        }
    }
    if order.r#type == OrderType::Conditional {
        let raw = order.info.as_ref();
        let id = raw
            .and_then(|r| r.get("algoId"))
            .filter(|v| v.is_truthy())
            .cloned()
            .unwrap_or_else(|| Input::Text(Vec::new()));
        if !raw
            .and_then(|r| r.get("orderType"))
            .is_some_and(|v| v.text_equals("trigger"))
            || safe_cex_num::js_retained_string(Some(&id)).map_err(ServiceError::Coercion)?
                != order.id
        {
            reject(host.service,Some(&Value::String("Cannot safely update a BloFin trigger without authoritative trigger-order provenance".into())))?;
        }
        let mut opts = from_order(order);
        opts.amount = if object.contains_key("amount") {
            js_number(object.get("amount")).map_err(ServiceError::Coercion)?
        } else {
            opts.amount
        };
        let price = safe_cex_num::js_retained_number(raw.and_then(|r| r.get("orderPrice")))
            .map_err(ServiceError::Coercion)?;
        opts.price = (price.is_finite() && price > 0.0).then_some(price);
        opts.trigger_price = if object.contains_key("price") {
            Some(js_number(object.get("price")).map_err(ServiceError::Coercion)?)
        } else {
            order.price.into_value()
        };
        opts.reduce_only = false.into();
        opts.time_in_force = Some(OrderTimeInForce::GoodTillCancel);
        let attached = raw
            .and_then(|r| r.get("attachAlgoOrders"))
            .and_then(Input::as_array)
            .and_then(|a| a.first());
        let positive = |key| -> Result<Option<f64>, ServiceError<E>> {
            let value = safe_cex_num::js_retained_number(attached.and_then(|r| r.get(key)))
                .map_err(ServiceError::Coercion)?;
            Ok((value.is_finite() && value > 0.0).then_some(value))
        };
        opts.stop_loss = positive("slTriggerPrice")?;
        opts.take_profit = positive("tpTriggerPrice")?;
        let trigger = |key| {
            let value = attached.and_then(|r| r.get(key));
            if value.is_some_and(|v| v.text_equals("last")) {
                Some(safe_cex_types::OrderStopTriggerType::LastPrice)
            } else if value.is_some_and(|v| v.text_equals("mark")) {
                Some(safe_cex_types::OrderStopTriggerType::MarkPrice)
            } else if value.is_some_and(|v| v.text_equals("index")) {
                Some(safe_cex_types::OrderStopTriggerType::IndexPrice)
            } else {
                None
            }
        };
        opts.sl_trigger_type = trigger("slTriggerPriceType");
        opts.tp_trigger_type = trigger("tpTriggerPriceType");
        opts.trigger_type = None;
        opts.close = None;
        let payload = (host.format_trigger_order)(opts, raw.cloned()).await?;
        cancel_trigger_orders(std::slice::from_ref(order), store, xhr, host).await?;
        return Ok(vec![Some(Value::String(
            crate::order_services::place_trigger_order(&payload, xhr, host.service).await?,
        ))]);
    }
    let root = order.id.split("__").next().unwrap_or("");
    let related: Vec<_> = store
        .orders()?
        .iter()
        .filter(|o| o.id.starts_with(root))
        .cloned()
        .collect();
    let mut opts = from_order(order);
    let price = object
        .get("price")
        .filter(|v| !v.is_null())
        .map(|v| js_number(Some(v)).map_err(ServiceError::Coercion))
        .transpose()?;
    let amount = object
        .get("amount")
        .filter(|v| !v.is_null())
        .map(|v| js_number(Some(v)).map_err(ServiceError::Coercion))
        .transpose()?;
    opts.price = price.or(opts.price);
    opts.amount = amount.unwrap_or(opts.amount);
    if related.len() > 1 {
        let parent = related.iter().find(|o| !o.id.contains("__"));
        let sl = related.iter().find(|o| o.id.ends_with("__stop_loss"));
        let tp = related.iter().find(|o| o.id.ends_with("__take_profit"));
        if let Some(parent) = parent.filter(|o| o.status == OrderStatus::Open) {
            opts.r#type = parent.r#type.into_property();
            opts.side = parent.side.into_property();
            opts.amount = parent
                .extra
                .as_ref()
                .and_then(|e| e.remaining_native)
                .unwrap_or(parent.amount);
            opts.price = parent.price.into_value();
            opts.reduce_only = parent.reduce_only.into_property();
        } else {
            opts.price = None;
            if order.id.ends_with("__take_profit") {
                opts.r#type = OrderType::TakeProfit.into();
            } else if order.id.ends_with("__stop_loss") {
                opts.r#type = OrderType::StopLoss.into();
            }
        }
        if let Some(sl) = sl {
            if truth_number((sl.price).to_number()) {
                opts.stop_loss = Some((sl.price).to_number());
            }
            if sl.sl_trigger_type.is_some() {
                opts.sl_trigger_type = (sl.sl_trigger_type).into_value();
            }
        }
        if let Some(tp) = tp {
            if truth_number((tp.price).to_number()) {
                opts.take_profit = Some((tp.price).to_number());
            }
            if tp.tp_trigger_type.is_some() {
                opts.tp_trigger_type = (tp.tp_trigger_type).into_value();
            }
        }
        if let Some(price) = price {
            if order.id.ends_with("__stop_loss") {
                opts.stop_loss = Some(price);
            } else if order.id.ends_with("__take_profit") {
                opts.take_profit = Some(price);
            } else {
                opts.price = Some(price);
            }
        }
    }
    if !order.id.contains("__") && order.r#type != OrderType::Limit {
        cancel_algo_orders(&related, store, xhr, host).await?;
    } else {
        cancel_orders(&related, store, xhr, host).await?;
    }
    (host.place_order)(opts).await
}
