//! Mode/leverage reads and selected writes on configured shared REST adapters.
use crate::{
    modes::{AccountModes, apply_confirmed_position_mode},
    order_formatters::OrderPayload,
    types::ENDPOINTS,
    utility::UtilityInput,
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::{
    js_string, js_truthy,
    numeric_ingress::{js_from_str, js_from_value_ref},
    parse_float,
};
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_transport::{RestAdapter, RestBody, RestFailure, RestRequest, RestResponseBody};
use safe_cex_types::{FetchedLeverage, MarginType, Order, PositionMode};
use serde_json::{Value, json};
use std::future::Future;
#[derive(Debug)]
pub enum ServiceError<E> {
    Host(E),
    Rest(Box<RestFailure>),
    Ingress(serde_json::Error),
    Coercion(safe_cex_num::JsCoercionError),
    Store(StoreMutationError),
    Input(String),
}
type Emitter<'a, E> = Box<dyn Fn(Option<&Value>) -> Result<(), E> + 'a>;
type Logger<'a, E> = Box<dyn Fn(&str, LogSeverity) -> Result<(), E> + 'a>;
pub struct ServiceHost<'a, E> {
    pub emit_and_log_error: Emitter<'a, E>,
    pub log: Logger<'a, E>,
}
pub(crate) fn body<E>(body: &RestResponseBody) -> Result<UtilityInput, ServiceError<E>> {
    match body {
        RestResponseBody::Encoded(bytes) => js_from_str(&String::from_utf8_lossy(bytes)),
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => value
            .clone()
            .into_value()
            .and_then(|value| js_from_value_ref(&value)),
    }
    .map_err(ServiceError::Ingress)
}
pub(crate) async fn request<E>(
    xhr: &dyn RestAdapter,
    method: &str,
    endpoint: &str,
    params: Option<Value>,
    payload: Option<Value>,
) -> Result<UtilityInput, ServiceError<E>> {
    let req = RestRequest {
        method: method.into(),
        url: ENDPOINTS[endpoint]
            .as_str()
            .expect("static endpoint")
            .into(),
        params,
        body: payload
            .map(safe_cex_types::retained_json::RetainedJsonInput::from_value)
            .transpose()
            .map_err(ServiceError::Ingress)?
            .map(RestBody::Untransformed),
        ..Default::default()
    };
    let response = xhr
        .execute(req)
        .await
        .map_err(|error| ServiceError::Rest(Box::new(error)))?;
    body(&response.body)
}
pub(crate) fn field<'a, E>(
    value: &'a Value,
    key: &str,
) -> Result<Option<&'a Value>, ServiceError<E>> {
    if value.is_null() {
        Err(ServiceError::Input(format!(
            "Cannot read properties of null (reading '{key}')"
        )))
    } else {
        Ok(value.get(key))
    }
}
pub(crate) fn code_ok<E>(data: &Value) -> Result<bool, ServiceError<E>> {
    Ok(field(data, "code")?.and_then(Value::as_str) == Some("0"))
}
pub(crate) fn emit<E>(
    host: &ServiceHost<'_, E>,
    value: Option<&Value>,
) -> Result<(), ServiceError<E>> {
    (host.emit_and_log_error)(value).map_err(ServiceError::Host)
}
fn log<E>(
    host: &ServiceHost<'_, E>,
    text: &str,
    severity: LogSeverity,
) -> Result<(), ServiceError<E>> {
    (host.log)(text, severity).map_err(ServiceError::Host)
}
pub async fn get_or_fetch_position_mode<E>(
    symbol: Option<&str>,
    refetch: bool,
    modes: &AccountModes,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<PositionMode, ServiceError<E>> {
    if let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty())
        && let Some(mode) = store
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .and_then(|p| (p.position_mode).into_value())
    {
        return Ok(mode);
    }
    if !refetch && let Some(mode) = modes.position_mode.get() {
        return Ok(mode);
    }
    let data = request(xhr, "GET", "POSITION_MODE", None, None).await?;
    let data = data.json();
    if !code_ok(data)? {
        emit(host, data.get("msg"))?;
        return Ok(PositionMode::OneWay);
    }
    let row = data.get("data").ok_or_else(|| {
        ServiceError::Input("Cannot read properties of undefined (reading 'positionMode')".into())
    })?;
    let mode = if field(row, "positionMode")?.and_then(Value::as_str) == Some("net_mode") {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    };
    modes.position_mode.set(Some(mode));
    Ok(mode)
}
pub async fn get_or_fetch_margin_type<E>(
    symbol: Option<&str>,
    refetch: bool,
    modes: &AccountModes,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<MarginType, ServiceError<E>> {
    if let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty())
        && let Some(mode) = store
            .positions()
            .iter()
            .find(|p| p.symbol == symbol && p.contracts > 0.0)
            .and_then(|p| (p.margin_type).into_value())
    {
        return Ok(mode);
    }
    if !refetch && let Some(mode) = modes.margin_type.get() {
        return Ok(mode);
    }
    let data = request(xhr, "GET", "MARGIN_MODE", None, None).await?;
    let data = data.json();
    if !code_ok(data)? {
        emit(host, data.get("msg"))?;
        return Ok(MarginType::Cross);
    }
    let row = data.get("data").ok_or_else(|| {
        ServiceError::Input("Cannot read properties of undefined (reading 'marginMode')".into())
    })?;
    let mode = if field(row, "marginMode")?.and_then(Value::as_str) == Some("isolated") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    };
    modes.margin_type.set(Some(mode));
    Ok(mode)
}
fn parse<E>(value: Option<&Value>) -> Result<f64, ServiceError<E>> {
    Ok(parse_float(
        &js_string(value).map_err(ServiceError::Coercion)?,
    ))
}
pub(crate) fn error_message<E: std::fmt::Display>(
    error: &ServiceError<E>,
) -> Result<String, ServiceError<E>> {
    match error {
        ServiceError::Rest(error) => {
            let msg = error
                .response
                .as_ref()
                .and_then(|r| body::<E>(&r.body).ok())
                .and_then(|r| r.json().get("msg").filter(|v| js_truthy(Some(v))).cloned());
            Ok(match msg {
                Some(msg) => js_string(Some(&msg)).map_err(ServiceError::Coercion)?,
                None => error.message.clone(),
            })
        }
        ServiceError::Input(message) => Ok(message.clone()),
        ServiceError::Ingress(error) => Ok(error.to_string()),
        ServiceError::Coercion(error) => Ok(error.message.into()),
        ServiceError::Store(error) => Ok(error.to_string()),
        ServiceError::Host(error) => Ok(error.to_string()),
    }
}
fn find_side<'a, E>(rows: &'a [Value], side: &str) -> Result<Option<&'a Value>, ServiceError<E>> {
    for row in rows {
        if field(row, "positionSide")?.and_then(Value::as_str) == Some(side) {
            return Ok(Some(row));
        }
    }
    Ok(None)
}
pub async fn fetch_leverage_for_symbol<E: std::fmt::Display>(
    symbol: &str,
    modes: &AccountModes,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<Option<FetchedLeverage>, ServiceError<E>> {
    let operation=async {
        let market=store.markets_by_symbol().get(symbol).cloned();
        let Some(market)=market else {log(host,&format!("Market {symbol} not found"),LogSeverity::Warning)?;return Ok(None);};
        let margin=get_or_fetch_margin_type(None,false,modes,store,xhr,host).await?;
        let data=request(xhr,"GET","LEVERAGE",Some(json!({"instId":market.id,"marginMode":if margin==MarginType::Isolated {"isolated"}else{"cross"}})),None).await?;let data=data.json();
        if !code_ok(data)? {emit(host,data.get("msg"))?;return Ok(None);}
        let rows=data.get("data").filter(|v|js_truthy(Some(v)));let Some(rows)=rows else{return Ok(None)};
        // Source checks rows.length===0 before calling find, even on JSON objects.
        if rows.get("length").and_then(Value::as_f64)==Some(0.0){return Ok(None);}
        let rows=rows.as_array().ok_or_else(||ServiceError::Input("rows.find is not a function".into()))?;let Some(first)=rows.first() else{return Ok(None)};
        let net=find_side(rows,"net")?;
        let long=find_side(rows,"long")?.or(net).unwrap_or(first);
        let short=find_side(rows,"short")?.or(net).unwrap_or(first);
        let buy=parse(field(long,"leverage")?)?;let sell=parse(field(short,"leverage")?)?;
        if !buy.is_finite()||!sell.is_finite(){return Ok(None);}
        let margin=long.get("marginMode").filter(|v|js_truthy(Some(v))).or_else(||short.get("marginMode").filter(|v|js_truthy(Some(v)))).or_else(||net.and_then(|n|n.get("marginMode")));
        let leverage=FetchedLeverage {buy_leverage:buy,sell_leverage:sell,margin_type:Some(if margin.and_then(Value::as_str)==Some("isolated"){MarginType::Isolated}else{MarginType::Cross}),position_mode:Some(if net.is_some(){PositionMode::OneWay}else{PositionMode::Hedged}),is_fetching:None};
        let mut fetched=store.fetched_leverage().clone();fetched.insert(symbol.into(),leverage.clone());store.update(StoreUpdate {fetched_leverage:Some(fetched),..Default::default()},false).map_err(ServiceError::Store)?;Ok(Some(leverage))
    }.await;
    match operation {
        Ok(value) => Ok(value),
        Err(error) => {
            let message = error_message(&error)?;
            log(
                host,
                &format!("Error fetching leverage for {symbol}: {message}"),
                LogSeverity::Warning,
            )?;
            Ok(None)
        }
    }
}
pub async fn change_position_mode<E>(
    hedged: bool,
    modes: &AccountModes,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<(), ServiceError<E>> {
    let target = if hedged {
        "long_short_mode"
    } else {
        "net_mode"
    };
    log(
        host,
        &format!("Attempting to change position mode to {target}"),
        LogSeverity::Info,
    )?;
    log(
        host,
        &format!(
            "Current positions count: {}",
            store
                .positions()
                .iter()
                .filter(|p| p.contracts > 0.0)
                .count()
        ),
        LogSeverity::Info,
    )?;
    log(
        host,
        &format!("Current orders count: {}", store.orders().len()),
        LogSeverity::Info,
    )?;
    let data = request(
        xhr,
        "POST",
        "SET_POSITION_MODE",
        None,
        Some(json!({"positionMode":target})),
    )
    .await?;
    let data = data.json();
    if !code_ok(data)? {
        let msg = js_string(data.get("msg")).map_err(ServiceError::Coercion)?;
        let code = js_string(data.get("code")).map_err(ServiceError::Coercion)?;
        log(
            host,
            &format!("Failed to change position mode: {msg} (code: {code})"),
            LogSeverity::Error,
        )?;
        let message = format!("{msg} (BloFin error code: {code})");
        emit(host, Some(&Value::String(message.clone())))?;
        return Err(ServiceError::Input(message));
    }
    let confirmed = data.get("data").and_then(|d| d.get("positionMode"));
    let mode = if confirmed.and_then(Value::as_str) == Some("net_mode") {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    };
    apply_confirmed_position_mode(modes, store, mode).map_err(ServiceError::Store)?;
    let text = match confirmed.filter(|v| !v.is_null()) {
        Some(value) => js_string(Some(value)).map_err(ServiceError::Coercion)?,
        None => target.into(),
    };
    log(
        host,
        &format!("Position mode change successful: {text}"),
        LogSeverity::Info,
    )?;
    Ok(())
}
pub async fn place_trigger_order<E>(
    payload: &OrderPayload,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<String, ServiceError<E>> {
    let data = request(
        xhr,
        "POST",
        "PLACE_TRIGGER_ORDER",
        None,
        Some(Value::Object(payload.fields.clone())),
    )
    .await?;
    let data = data.json();
    let row = data.get("data");
    let message = row
        .and_then(|r| r.get("msg"))
        .filter(|m| js_truthy(Some(m)))
        .or_else(|| data.get("msg").filter(|m| js_truthy(Some(m))))
        .cloned()
        .unwrap_or_else(|| {
            Value::String("BloFin trigger placement returned an invalid response".into())
        });
    let code = row.and_then(|r| r.get("code"));
    let id = row.and_then(|r| r.get("algoId"));
    if data.get("code").and_then(Value::as_str) != Some("0")
        || code.is_some_and(|c| c.as_str() != Some("0"))
        || !js_truthy(id)
    {
        emit(host, Some(&message))?;
        return Err(ServiceError::Input(
            js_string(Some(&message)).map_err(ServiceError::Coercion)?,
        ));
    }
    js_string(id).map_err(ServiceError::Coercion)
}
pub async fn cancel_symbol_orders<E, F: Future<Output = Result<(), E>>>(
    symbol: &str,
    store: &dyn Store,
    cancel_orders: impl FnOnce(Vec<Order>) -> F,
) -> Result<(), E> {
    let orders = store
        .orders()
        .iter()
        .filter(|o| o.symbol == symbol)
        .cloned()
        .collect();
    cancel_orders(orders).await
}
