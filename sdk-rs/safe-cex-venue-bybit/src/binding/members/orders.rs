//! Order placement: placeOrder and its option, stop and trailing branches, the
//! create-order submission and the position-index helpers they await.
use crate::{
    binding::{
        host::{get_key, text},
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, error_object, from_json, from_typed, to_json, to_typed},
    },
    order_params::BybitPlaceOrderOpts,
};
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{OrderType, PlaceOrderOpts};
use serde_json::{Map, Value};
use std::{borrow::Borrow, rc::Rc};

fn num(n: f64) -> Js {
    Js::str(&js_number_string(n))
}

fn reject(rt: &Rt, message: &str) -> Thrown {
    // ts: BaseExchange.rejectPreflight — emit first, then throw.
    if let Err(error) = rt
        .this
        .call_method("emitAndLogError", vec![Js::str(message)])
    {
        return error;
    }
    Thrown::new(error_object("Error", message))
}

fn object(fields: Vec<(&str, Js)>) -> Js {
    Js::object(
        fields
            .into_iter()
            .filter(|(_, v)| !v.is_undefined())
            .collect(),
    )
}

/// `inverseObj(TABLE)[value]`: the last key mapping to a value wins.
fn inverse(table: &Value, value: &str) -> Js {
    table
        .as_object()
        .into_iter()
        .flatten()
        .rfind(|(_, v)| v.as_str() == Some(value))
        .map_or(Js::Undefined, |(k, _)| Js::str(k))
}

fn kind_text(kind: OrderType) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

async fn post(rt: &Rt, endpoint: &str, payload: Js) -> Result<Js, Thrown> {
    let xhr = rt.this.get("xhr")?;
    await_js(
        &rt.exchange_runtime(),
        xhr.call_method("post", vec![Js::str(endpoint), payload])?,
    )
    .await
}

fn format_venue_error(rt: &Rt, data: &Js, fallback: &Js) -> Result<String, Thrown> {
    let name = text(&rt.this.get("name")?)?;
    Ok(crate::exchange_errors::format_bybit_venue_error(
        &to_json(data)?,
        &to_json(fallback)?,
        Some(&name),
    ))
}

fn emit(rt: &Rt, message: &str) -> Result<(), Thrown> {
    rt.this
        .call_method("emitAndLogError", vec![Js::str(message)])?;
    Ok(())
}

fn market(rt: &Rt, symbol: &str) -> Result<Option<safe_cex_types::Market>, Thrown> {
    let venue = venue(rt)?;
    Ok(venue
        .base
        .store
        .markets()?
        .iter()
        .find(|m| m.symbol == symbol)
        .cloned())
}

// ts: BybitExchange.submitCreateOrders — sequential; a failed send is reported and
// dropped, except a lost response on a single order carrying the caller's id.
pub(crate) async fn submit_create_orders_body(
    rt: &Rt,
    payloads: Vec<Js>,
    opts: &Js,
) -> Result<Vec<Js>, Thrown> {
    let single = payloads.len() == 1;
    let mut responses = vec![];
    for payload in payloads {
        let endpoint = crate::types::ENDPOINTS["CREATE_ORDER"].as_str().unwrap();
        match post(rt, endpoint, payload.clone()).await {
            Ok(response) => responses.push(Js::object(vec![
                ("data", response.get("data")?),
                ("payload", payload),
            ])),
            Err(error) => {
                let data = error
                    .value
                    .get("response")
                    .and_then(|r| {
                        if r.is_nullish() {
                            Ok(Js::Undefined)
                        } else {
                            r.get("data")
                        }
                    })
                    .unwrap_or(Js::Undefined);
                let message = error.value.get("message").unwrap_or(Js::Undefined);
                emit(rt, &format_venue_error(rt, &data, &message)?)?;
                if single
                    && opts.get("clientOrderId")?.truthy()
                    && safe_cex_store::remaining_utils::is_unknown_outcome(&to_json(&error.value)?)
                        .map_err(|m| Thrown::error("TypeError", &m))?
                {
                    return Err(error);
                }
            }
        }
    }
    Ok(responses)
}

/// The responses' order ids, failing on the first non-OK body as the source does.
fn collect_ids(rt: &Rt, opts: &Js, responses: Vec<Js>) -> Result<Js, Thrown> {
    let mut error = String::new();
    for response in &responses {
        let data = response.get("data")?;
        if !get_key(&data, "retMsg")?.strict_equals(&Js::str("OK")) {
            let formatted = format_venue_error(rt, &data, &Js::Undefined)?;
            emit(rt, &formatted)?;
            error = formatted;
        }
    }
    if !error.is_empty() {
        return Err(Thrown::new(error_object("Error", &error)));
    }
    let mut ids = vec![];
    for response in responses {
        let order_id = response.get("data")?.get("result")?.get("orderId")?;
        if order_id.truthy()
            && !response.get("terminal")?.truthy()
            && !response.get("recorded")?.truthy()
        {
            rt.this.call_method(
                "recordRecentlyPlacedOrderFromOpts",
                vec![order_id.clone(), opts.clone(), response.get("payload")?],
            )?;
        }
        ids.push(order_id);
    }
    Ok(Js::array(ids))
}

// ts: BybitExchange#assertOptionOrderPlaceable
pub(crate) fn assert_option_order_placeable(
    rt: &Rt,
    opts: &BybitPlaceOrderOpts,
) -> Result<(), Thrown> {
    let base: &PlaceOrderOpts = opts.borrow();
    if !venue(rt)?.options_enabled() {
        return Err(reject(rt, "Options are not enabled on this Bybit account"));
    }
    if crate::order_params::has_bybit_single_order_params(opts) {
        return Err(reject(
            rt,
            &format!(
                "Bybit execution parameters are not supported on option orders ({})",
                base.symbol
            ),
        ));
    }
    if !matches!(base.r#type, OrderType::Limit | OrderType::Market) {
        return Err(reject(
            rt,
            &format!(
                "Bybit options take limit and market orders only, not {} ({})",
                kind_text(base.r#type),
                base.symbol
            ),
        ));
    }
    for (field, present) in [
        ("triggerPrice", base.trigger_price.is_some()),
        ("stopLoss", base.stop_loss.is_some()),
        ("takeProfit", base.take_profit.is_some()),
    ] {
        if present {
            return Err(reject(
                rt,
                &format!("Bybit options do not support {field} ({})", base.symbol),
            ));
        }
    }
    if market(rt, &base.symbol)?.is_none() {
        return Err(reject(rt, &format!("Market {} not found", base.symbol)));
    }
    let by_iv = base.implied_volatility.is_some();
    if base.r#type == OrderType::Market && by_iv {
        return Err(reject(
            rt,
            &format!(
                "Bybit takes impliedVolatility on limit option orders only ({})",
                base.symbol
            ),
        ));
    }
    if base.r#type == OrderType::Limit
        && !by_iv
        && !base.price.is_some_and(|p| p != 0. && !p.is_nan())
    {
        return Err(reject(
            rt,
            &format!(
                "A limit option order needs a price or an impliedVolatility ({})",
                base.symbol
            ),
        ));
    }
    Ok(())
}

fn time_in_force(base: &PlaceOrderOpts) -> Js {
    let tif = base
        .time_in_force
        .and_then(|t| {
            serde_json::to_value(t)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| "GoodTillCancel".into());
    crate::types::ORDER_TIME_IN_FORCE_OUTBOUND
        .get(&tif)
        .map_or(Js::Undefined, from_json)
}

// ts: BybitExchange#placeOptionOrder
async fn place_option_order(
    rt: &Rt,
    opts: &Js,
    typed: &BybitPlaceOrderOpts,
    client_order_id: Js,
) -> Result<Js, Thrown> {
    assert_option_order_placeable(rt, typed)?;
    let base: &PlaceOrderOpts = typed.borrow();
    let market = market(rt, &base.symbol)?.expect("checked by assertOptionOrderPlaceable");
    let is_limit = base.r#type == OrderType::Limit;
    let by_iv = base.implied_volatility.is_some();
    let link_id = if client_order_id.is_undefined() {
        Js::str(&rt.exchange_runtime().unique_id())
    } else {
        client_order_id
    };
    let payload = object(vec![
        ("category", Js::str("option")),
        ("symbol", Js::str(&base.symbol)),
        ("side", inverse(&crate::types::ORDER_SIDE, &kind_side(base))),
        (
            "orderType",
            inverse(&crate::types::ORDER_TYPE, &kind_text(base.r#type)),
        ),
        ("qty", num(adjust(base.amount, market.precision.amount))),
        (
            "price",
            if is_limit && !by_iv {
                num(adjust(
                    base.price.unwrap_or(f64::NAN),
                    market.precision.price,
                ))
            } else {
                Js::Undefined
            },
        ),
        (
            "orderIv",
            base.implied_volatility.map_or(Js::Undefined, num),
        ),
        (
            "timeInForce",
            if is_limit {
                time_in_force(base)
            } else {
                Js::Undefined
            },
        ),
        ("reduceOnly", Js::Bool(base.reduce_only.unwrap_or(false))),
        ("orderLinkId", link_id),
    ]);
    let responses = await_js(
        &rt.exchange_runtime(),
        rt.this.call_method(
            "submitCreateOrders",
            vec![Js::array(vec![payload]), opts.clone()],
        )?,
    )
    .await?;
    let responses = responses
        .array_values()?
        .ok_or_else(|| Thrown::type_error("submitCreateOrders did not return an array"))?;
    let Some(response) = responses.first().cloned() else {
        return Err(Thrown::new(error_object(
            "Error",
            &format!("Bybit did not answer the option order for {}", base.symbol),
        )));
    };
    collect_ids(rt, opts, vec![response])
}

fn kind_side(base: &PlaceOrderOpts) -> String {
    serde_json::to_value(base.side)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

// ts: getBybitResponseMeta and its normalization helpers. Retained status
// values share the receiver's JSON.parse capability for raw proxy envelopes.
pub(crate) fn position_response_meta(rt: &Rt, data: &Js) -> Result<Js, Thrown> {
    let envelope = if data.is_object() || data.is_array() {
        data.clone()
    } else {
        Js::object(vec![])
    };
    let raw = envelope.get("raw_response")?;
    let raw = if raw.is_nullish() {
        envelope.get("rawResponse")?
    } else {
        raw
    };
    let raw = if matches!(raw, Js::String(_)) {
        rt.global("JSON")?
            .call_method("parse", vec![raw])
            .unwrap_or(Js::Undefined)
    } else {
        raw
    };
    let raw = if raw.is_object() && !raw.is_array() {
        raw
    } else {
        Js::object(vec![])
    };
    let mut code = Js::Undefined;
    let mut message = Js::Undefined;
    for candidate in [&envelope, &raw] {
        if code.is_undefined() {
            for key in ["retCode", "ret_code", "code"] {
                let value = candidate.get(key)?;
                let number = match &value {
                    Js::Number(number) => Some(*number),
                    Js::String(_) => {
                        let value = text(&value)?;
                        let trimmed = safe_cex_num::js_trim(&value);
                        (!trimmed.is_empty()).then(|| safe_cex_num::number_from_string(trimmed))
                    }
                    _ => None,
                };
                if let Some(number) = number.filter(|n| n.is_finite()) {
                    code = Js::Number(number);
                    break;
                }
            }
        }
        if message.is_undefined() {
            for key in ["retMsg", "ret_msg", "message", "msg"] {
                let value = candidate.get(key)?;
                if matches!(value, Js::String(_)) {
                    let trimmed = value.call_method("trim", vec![])?;
                    if trimmed.truthy() {
                        message = trimmed;
                        break;
                    }
                }
            }
        }
    }
    Ok(Js::object(vec![
        (
            "isSuccess",
            Js::Bool(if code.is_undefined() {
                message.strict_equals(&Js::str("OK"))
            } else {
                code.strict_equals(&Js::Number(0.))
            }),
        ),
        (
            "hasExplicitStatus",
            Js::Bool(!code.is_undefined() || !message.is_undefined()),
        ),
        ("retCode", code),
        ("retMsg", message),
    ]))
}

// ts: BybitExchange.fetchPositionMode. Mapping and Store publication never
// project a native Position through JSON.
pub(crate) fn fetch_position_mode(rt: &Rt, symbol: Js) -> Result<Js, Thrown> {
    fetch_position_mode_body(rt, symbol, true)
}

// ts: BybitInverseExchange.fetchPositionMode — direct retMsg policy and
// optional map, rather than the primary normalized-status/array-only policy.
pub(crate) fn fetch_inverse_position_mode(rt: &Rt, symbol: Js) -> Result<Js, Thrown> {
    fetch_position_mode_body(rt, symbol, false)
}

fn fetch_position_mode_body(rt: &Rt, symbol: Js, primary: bool) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        let store = rt.this.get("store")?;
        let selected_symbol = symbol.clone();
        let found = store.get("positions")?.call_method(
            "find",
            vec![Js::function("positionModeSymbol", move |_, args| {
                Ok(Js::Bool(
                    crate::binding::value::arg(&args, 0)
                        .get("symbol")?
                        .strict_equals(&selected_symbol),
                ))
            })],
        )?;
        let mode = if found.is_nullish() {
            Js::Undefined
        } else {
            found.get("positionMode")?
        };
        if mode.truthy() {
            return Ok(mode);
        }
        let endpoint = crate::types::ENDPOINTS["POSITIONS"].as_str().unwrap();
        let data = await_js(
            &rt.exchange_runtime(),
            rt.this.get("xhr")?.call_method(
                "get",
                vec![
                    Js::str(endpoint),
                    Js::object(vec![(
                        "params",
                        Js::object(vec![
                            (
                                "category",
                                rt.this
                                    .call_method("getCategoryForSymbol", vec![symbol.clone()])?,
                            ),
                            ("symbol", symbol.clone()),
                        ]),
                    )]),
                ],
            )?,
        )
        .await?
        .get("data")?;
        let meta = if primary {
            position_response_meta(&rt, &data)?
        } else {
            Js::Undefined
        };
        if !primary && !get_key(&data, "retMsg")?.strict_equals(&Js::str("OK")) {
            rt.this
                .call_method("emitAndLogError", vec![get_key(&data, "retMsg")?])?;
            return Ok(Js::str("oneway"));
        }
        let result = if data.is_nullish() {
            Js::Undefined
        } else {
            data.get("result")?
        };
        let list = if result.is_nullish() {
            Js::Undefined
        } else {
            result.get("list")?
        };
        let rows = list.array_items()?.unwrap_or_default();
        if primary && !meta.get("isSuccess")?.truthy() && rows.is_empty() {
            let message = meta.get("retMsg")?;
            let code = meta.get("retCode")?;
            let fallback = if message.truthy() {
                message
            } else if !code.is_undefined() {
                Js::str(&format!(
                    "Bybit positions request failed with code {}",
                    text(&code)?
                ))
            } else {
                Js::str("Bybit positions request returned malformed status payload")
            };
            rt.this.call_method(
                "emitAndLogError",
                vec![
                    fallback,
                    Js::str("Bybit fetchPositionMode failed"),
                    Js::object(vec![
                        ("endpoint", Js::str(endpoint)),
                        ("symbol", symbol),
                        ("retCode", code),
                        ("retMsg", meta.get("retMsg")?),
                        ("hasExplicitStatus", meta.get("hasExplicitStatus")?),
                    ]),
                ],
            )?;
            return Ok(Js::str("oneway"));
        }
        if !primary && !list.is_nullish() && !list.is_array() {
            return Err(Thrown::type_error(
                "data?.result?.list?.map is not a function",
            ));
        }
        let mapped = if !primary && list.is_nullish() {
            Js::Undefined
        } else {
            Js::array(
                rows.into_iter()
                    .map(|row| {
                        rt.this
                            .call_method("mapPosition", vec![row.unwrap_or(Js::Undefined)])
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };
        store.call_method("addOrUpdatePositions", vec![mapped.clone()])?;
        if mapped.get("length")?.truthy() {
            mapped.get("0")?.get("positionMode")
        } else {
            Ok(Js::str("oneway"))
        }
    })))
}

// ts: BybitExchange#getOrderPositionIdx
pub(crate) async fn get_order_position_idx(
    rt: &Rt,
    opts: Map<String, Value>,
) -> Result<f64, Thrown> {
    let opts = std::cell::RefCell::new(opts);
    crate::execution_guards::get_order_position_idx(&opts, |symbol| {
        let pending = rt.this.call_method(
            "fetchPositionMode",
            vec![symbol.as_ref().map_or(Js::Undefined, from_json)],
        );
        async move {
            let mode = await_js(&rt.exchange_runtime(), pending?).await?;
            Ok::<_, Thrown>(Some(to_json(&mode)?).filter(|v| !v.is_null()))
        }
    })
    .await
}

// ts: BybitExchange#getStopOrderPositionIdx
async fn get_stop_order_position_idx(rt: &Rt, opts: &Map<String, Value>) -> Result<Js, Thrown> {
    let index = crate::execution_guards::get_stop_order_position_idx(opts, |opts| {
        get_order_position_idx(rt, opts)
    })
    .await?;
    Ok(index.map_or(Js::Undefined, Js::Number))
}

/// The SET_TRADING_STOP answer: not-modified and OK both schedule the order refresh.
async fn trading_stop(rt: &Rt, payload: Js) -> Result<Js, Thrown> {
    let endpoint = crate::types::ENDPOINTS["SET_TRADING_STOP"]
        .as_str()
        .unwrap();
    let data = post(rt, endpoint, payload).await?.get("data")?;
    let message = data.get("retMsg")?;
    let not_modified =
        crate::exchange_helpers::is_bybit_order_not_modified_message(Some(&to_json(&message)?))
            .map_err(|m| Thrown::error("TypeError", &m))?;
    if !not_modified && !message.strict_equals(&Js::str("OK")) {
        let formatted = format_venue_error(rt, &data, &Js::Undefined)?;
        emit(rt, &formatted)?;
        return Err(Thrown::new(error_object("Error", &formatted)));
    }
    rt.this
        .call_method("scheduleSetTradingStopSnapshotRefresh", vec![])?;
    Ok(Js::array(vec![]))
}

// ts: BybitExchange.placeStopLossOrTakeProfit
pub(crate) async fn place_stop_loss_or_take_profit(rt: &Rt, opts: &Js) -> Result<Js, Thrown> {
    let base: PlaceOrderOpts = to_typed(opts)?;
    let Some(market) = market(rt, &base.symbol)? else {
        return Err(Thrown::new(error_object(
            "Error",
            &format!("Market {} not found", base.symbol),
        )));
    };
    let amount = adjust(base.amount, market.precision.amount);
    let sized = (amount != 0. && !amount.is_nan()).then_some(amount);
    let opts_json = to_json(opts)?.as_object().cloned().unwrap_or_default();
    let payload = Js::object(vec![
        (
            "category",
            Js::str(venue(rt)?.get_category_for_symbol(&base.symbol)),
        ),
        ("symbol", Js::str(&base.symbol)),
        (
            "positionIdx",
            get_stop_order_position_idx(rt, &opts_json).await?,
        ),
        (
            "tpslMode",
            Js::str(if sized.is_some() { "Partial" } else { "Full" }),
        ),
    ]);
    let price = num(base.price.unwrap_or(f64::NAN));
    let trigger = |t: Option<safe_cex_types::OrderStopTriggerType>, last_default: bool| {
        use safe_cex_types::OrderStopTriggerType as T;
        Js::str(match (t, last_default) {
            (Some(T::LastPrice), false) => "LastPrice",
            (_, false) => "MarkPrice",
            (Some(T::MarkPrice), true) => "MarkPrice",
            (_, true) => "LastPrice",
        })
    };
    if base.r#type == OrderType::StopLoss {
        payload.set("stopLoss", price.clone())?;
        payload.set("slTriggerBy", trigger(base.sl_trigger_type, false))?;
        if let Some(size) = sized {
            payload.set("slSize", num(size))?;
        }
    }
    if base.r#type == OrderType::TakeProfit {
        payload.set("takeProfit", price)?;
        payload.set("tpTriggerBy", trigger(base.tp_trigger_type, true))?;
        if let Some(size) = sized {
            payload.set("tpSize", num(size))?;
        }
    }
    trading_stop(rt, payload).await
}

// ts: BybitExchange.placeTrailingStopLoss
pub(crate) async fn place_trailing_stop_loss(rt: &Rt, opts: &Js) -> Result<Js, Thrown> {
    let base: PlaceOrderOpts = to_typed(opts)?;
    let venue = venue(rt)?;
    let ticker = venue
        .base
        .store
        .tickers()?
        .iter()
        .find(|t| t.symbol == base.symbol)
        .cloned();
    let (Some(ticker), Some(market)) = (ticker, market(rt, &base.symbol)?) else {
        return Err(Thrown::new(error_object(
            "Error",
            &format!("Ticker {} not found", base.symbol),
        )));
    };
    let price = base.price.unwrap_or(f64::NAN);
    let distance = adjust(
        safe_cex_num::math_max(ticker.last, price) - safe_cex_num::math_min(ticker.last, price),
        market.precision.price,
    );
    let opts_json = to_json(opts)?.as_object().cloned().unwrap_or_default();
    let payload = Js::object(vec![
        (
            "category",
            Js::str(venue.get_category_for_symbol(&base.symbol)),
        ),
        ("symbol", Js::str(&base.symbol)),
        (
            "positionIdx",
            get_stop_order_position_idx(rt, &opts_json).await?,
        ),
        ("trailingStop", num(distance)),
    ]);
    trading_stop(rt, payload).await
}

// ts: BybitExchange.placeOrder
pub(crate) fn place_order(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    let this = rt.this.clone();
    let client_order_id = this.call_method("resolveCallerClientOrderId", vec![opts.clone()])?;
    let typed: BybitPlaceOrderOpts = to_typed(&opts)?;
    venue(rt)?
        .validate_order_capabilities(&typed)
        .map_err(Thrown::from)?;
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let venue = venue(&rt)?;
        let base: PlaceOrderOpts = Borrow::<PlaceOrderOpts>::borrow(&typed).clone();
        let category = venue.get_category_for_symbol(&base.symbol);
        if category == "option" {
            return place_option_order(&rt, &opts, &typed, client_order_id).await;
        }
        let execution = crate::execution_guards::get_order_execution_params(
            &typed,
            |symbol| Ok(venue.get_category_for_symbol(symbol).into()),
            |message| emit(&rt, message).map_err(|e| Rc::new(e.to_exchange_error())),
        )
        .map_err(|e| Thrown::from((*e).clone()))?;
        if venue.has_full_sized_position_opposite_side(&base.symbol, base.side, base.amount)?
            && matches!(base.r#type, OrderType::StopLoss | OrderType::TakeProfit)
        {
            return place_stop_loss_or_take_profit(&rt, &opts).await;
        }
        if base.r#type == OrderType::TrailingStopLoss {
            return place_trailing_stop_loss(&rt, &opts).await;
        }
        let Some(market) = market(&rt, &base.symbol)? else {
            return Err(Thrown::new(error_object(
                "Error",
                &format!("Market {} not found", base.symbol),
            )));
        };
        let mut position_opts = to_json(&opts)?.as_object().cloned().unwrap_or_default();
        if base.r#type == OrderType::Conditional {
            position_opts.insert("reduceOnly".into(), Value::Bool(false));
        }
        let position_idx = get_order_position_idx(&rt, position_opts).await?;

        let max_size = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
        let (p_price, p_amount) = (market.precision.price, market.precision.amount);
        let amount = adjust(base.amount, p_amount);
        let truthy = |v: Option<f64>| v.filter(|n| *n != 0. && !n.is_nan());
        let price = truthy(base.price).map(|p| adjust(p, p_price));
        let stop_loss = truthy(base.stop_loss).map(|p| adjust(p, p_price));
        let take_profit = truthy(base.take_profit).map(|p| adjust(p, p_price));
        let trigger_price = truthy(base.trigger_price).map(|p| adjust(p, p_price));
        let trigger_direction = match truthy(trigger_price) {
            Some(trigger) => {
                let ticker = venue
                    .base
                    .store
                    .tickers()?
                    .iter()
                    .find(|t| t.symbol == base.symbol)
                    .cloned()
                    .ok_or_else(|| {
                        Thrown::type_error("Cannot read properties of undefined (reading 'last')")
                    })?;
                let compare =
                    crate::order_format::get_bybit_trigger_direction_price(&base, &ticker);
                Some(if trigger > compare { 1. } else { 2. })
            }
            _ => None,
        };
        let mut kind = base.r#type;
        if base.r#type == OrderType::Conditional {
            if truthy(trigger_price).is_none() {
                return Err(Thrown::new(error_object(
                    "Error",
                    &format!(
                        "Trigger price is required for conditional Bybit orders ({})",
                        base.symbol
                    ),
                )));
            }
            kind = if truthy(price).is_some() {
                OrderType::Limit
            } else {
                OrderType::Market
            };
        }
        if truthy(trigger_price).is_some()
            && matches!(base.r#type, OrderType::StopLoss | OrderType::TakeProfit)
        {
            kind = OrderType::Market;
        }
        let priced = base.r#type == OrderType::Limit
            || (base.r#type == OrderType::Conditional && truthy(price).is_some());
        let price_text = |p: Option<f64>| num(p.unwrap_or(0.));
        use safe_cex_types::OrderStopTriggerType as T;
        let req = vec![
            ("category", Js::str(category)),
            ("symbol", Js::str(&base.symbol)),
            (
                "side",
                inverse(&crate::types::ORDER_SIDE, &kind_side(&base)),
            ),
            (
                "orderType",
                inverse(&crate::types::ORDER_TYPE, &kind_text(kind)),
            ),
            ("qty", num(amount)),
            (
                "price",
                if execution.bbo_side_type.is_none() && priced {
                    match price {
                        Some(p) => num(p),
                        None => Js::str("null"),
                    }
                } else {
                    Js::Undefined
                },
            ),
            (
                "stopLoss",
                if truthy(base.stop_loss).is_some() {
                    price_text(stop_loss)
                } else {
                    Js::Undefined
                },
            ),
            (
                "takeProfit",
                if truthy(base.take_profit).is_some() {
                    price_text(take_profit)
                } else {
                    Js::Undefined
                },
            ),
            (
                "reduceOnly",
                Js::Bool(
                    base.r#type != OrderType::Conditional && base.reduce_only.unwrap_or(false),
                ),
            ),
            (
                "triggerPrice",
                if truthy(base.trigger_price).is_some() {
                    price_text(trigger_price)
                } else {
                    Js::Undefined
                },
            ),
            (
                "triggerDirection",
                trigger_direction.map_or(Js::Undefined, num),
            ),
            (
                "triggerBy",
                crate::order_format::get_bybit_trigger_by(&base).map_or(Js::Undefined, Js::str),
            ),
            (
                "slTriggerBy",
                if truthy(base.stop_loss).is_some() {
                    Js::str(if base.sl_trigger_type == Some(T::LastPrice) {
                        "LastPrice"
                    } else {
                        "MarkPrice"
                    })
                } else {
                    Js::Undefined
                },
            ),
            (
                "tpTriggerBy",
                if truthy(base.take_profit).is_some() {
                    Js::str(if base.tp_trigger_type == Some(T::MarkPrice) {
                        "MarkPrice"
                    } else {
                        "LastPrice"
                    })
                } else {
                    Js::Undefined
                },
            ),
            (
                "timeInForce",
                if priced {
                    time_in_force(&base)
                } else {
                    Js::Undefined
                },
            ),
            (
                "closeOnTrigger",
                Js::Bool(matches!(
                    base.r#type,
                    OrderType::StopLoss | OrderType::TakeProfit
                )),
            ),
            ("positionIdx", Js::Number(position_idx)),
        ];
        let execution = from_typed(&execution)?;
        let req_object = object(req);
        for (key, value) in execution.own_entries()? {
            req_object.set_key(&key, value)?;
        }

        let lots = if amount > max_size {
            (amount / max_size).ceil()
        } else {
            1.
        };
        let rest = if amount > max_size {
            adjust(amount % max_size, p_amount)
        } else {
            0.
        };
        let lot_size = adjust((amount - rest) / lots, p_amount);
        let link_id = || -> Result<Js, Thrown> {
            if !client_order_id.is_undefined() {
                return Ok(client_order_id.clone());
            }
            Ok(venue
                .generated_order_link_id()
                .map_err(Thrown::from)?
                .map_or(Js::Undefined, |id| Js::str(&id)))
        };
        // Lodash times returns no lots outside 1..=MAX_SAFE_INTEGER, including Infinity.
        let count = if (1. ..=9_007_199_254_740_991.).contains(&lots) {
            lots as usize
        } else {
            0
        };
        let mut payloads = vec![];
        for idx in 0..count {
            let payload = req_object.spread_object()?;
            if idx > 0 {
                for key in ["stopLoss", "takeProfit", "slTriggerBy", "tpTriggerBy"] {
                    payload.delete(&crate::binding::value::utf16(key))?;
                }
            }
            payload.set("qty", num(lot_size))?;
            payload.set("orderLinkId", link_id()?)?;
            payloads.push(payload);
        }
        if rest != 0. && !rest.is_nan() {
            let payload = req_object.spread_object()?;
            payload.set("qty", num(rest))?;
            payload.set("orderLinkId", link_id()?)?;
            payloads.push(payload);
        }
        rt.this.call_method(
            "assertSingleNativeClientOrderId",
            vec![client_order_id.clone(), Js::Number(payloads.len() as f64)],
        )?;
        let responses = await_js(
            &rt.exchange_runtime(),
            rt.this.call_method(
                "submitCreateOrders",
                vec![Js::array(payloads), opts.clone()],
            )?,
        )
        .await?;
        let responses = responses
            .array_values()?
            .ok_or_else(|| Thrown::type_error("submitCreateOrders did not return an array"))?;
        collect_ids(&rt, &opts, responses)
    })))
}
