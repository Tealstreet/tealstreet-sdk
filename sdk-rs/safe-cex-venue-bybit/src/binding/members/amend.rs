//! updateOrder: amend in place, cancel-and-replace for position TP/SL and trailing
//! stops, and the trading-stop fallback.
use super::orders::{
    get_order_position_idx, place_stop_loss_or_take_profit, place_trailing_stop_loss,
};
use crate::{
    binding::{
        host::text,
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, error_object, to_json},
    },
    exchange_helpers::is_bybit_position_tpsl_mode,
};

const STOP_LOSS: &str = "stop_market";
const TAKE_PROFIT: &str = "take_profit_market";

fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS[name].as_str().unwrap_or_default()
}

/// `` `${value}` ``.
fn template(value: &Js) -> Result<Js, Thrown> {
    Ok(Js::String(value.to_js_string()?))
}

/// `new Error(message)`.
fn error(message: &Js) -> Result<Thrown, Thrown> {
    let message = if message.is_undefined() {
        String::new()
    } else {
        text(message)?
    };
    Ok(Thrown::new(error_object("Error", &message)))
}

/// `{ ...a, ...b }`.
fn merge(a: &Js, b: &Js) -> Result<Js, Thrown> {
    let merged = a.spread_object()?;
    for (key, value) in b.own_entries()? {
        merged.set_key(&key, value)?;
    }
    Ok(merged)
}

async fn post(rt: &Rt, endpoint: &str, payload: Js) -> Result<Js, Thrown> {
    let xhr = rt.this.get("xhr")?;
    await_js(xhr.call_method("post", vec![Js::str(endpoint), payload])?)
        .await?
        .get("data")
}

fn emit(rt: &Rt, args: Vec<Js>) -> Result<(), Thrown> {
    rt.this.call_method("emitAndLogError", args)?;
    Ok(())
}

fn apply_local(rt: &Rt, order: &Js, update: Js, primary: bool) -> Result<(), Thrown> {
    if !primary {
        rt.this.call_method(
            "applyLocalOrderUpdateAfterRestWrite",
            vec![order.clone(), update],
        )?;
        return Ok(());
    }
    crate::primary_local_order_writes::apply_local_order_update_after_rest_write(
        rt,
        &rt.this,
        order.clone(),
        update,
    )
}

async fn cancel(rt: &Rt, order: &Js) -> Result<(), Thrown> {
    let pending = rt
        .this
        .call_method("cancelOrders", vec![Js::array(vec![order.clone()])])?;
    await_js(pending).await?;
    Ok(())
}

fn category(rt: &Rt, symbol: &str) -> Result<Js, Thrown> {
    rt.this
        .call_method("getCategoryForSymbol", vec![Js::str(symbol)])
}

/// `data.result.orderId`.
fn result_order_id(data: &Js) -> Result<Js, Thrown> {
    data.get("result")?.get("orderId")
}

// ts: BybitExchange.updateOrder
pub(crate) fn update_order(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    update_body(rt, opts, true)
}

// ts: BybitInverseExchange.updateOrder
pub(crate) fn update_inverse_order(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    update_body(rt, opts, false)
}

fn update_body(rt: &Rt, opts: Js, primary: bool) -> Result<Js, Thrown> {
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let order = opts.get("order")?;
        let update = opts.get("update")?;
        let id = text(&order.get("id")?)?;
        let symbol = text(&order.get("symbol")?)?;
        let kind = text(&order.get("type")?)?;
        let is_virtual = id.ends_with("__stop_loss") || id.ends_with("__take_profit");

        if kind == "trailing_stop_market" {
            cancel(&rt, &order).await?;
            let replacement = merge(&order, &update)?;
            let ids = if primary {
                place_trailing_stop_loss(&rt, &replacement).await?
            } else {
                await_js(
                    rt.this
                        .call_method("placeTrailingStopLoss", vec![replacement])?,
                )
                .await?
            };
            apply_local(&rt, &order, update, primary)?;
            return Ok(ids);
        }

        let info = order.get("info")?;
        let tpsl_mode = if info.is_nullish() {
            Js::Undefined
        } else {
            info.get("tpslMode")?
        };
        let position_tpsl = is_bybit_position_tpsl_mode(Some(&to_json(&tpsl_mode)?));
        let is_stop = kind == STOP_LOSS || kind == TAKE_PROFIT;

        if !is_virtual && is_stop && position_tpsl && (primary || update.has_own("price")) {
            let mark = venue(&rt)?
                .base
                .store
                .tickers()
                .iter()
                .find(|t| t.symbol == symbol)
                .map(|t| t.mark);
            if let Some(mark) = mark.filter(|m| *m != 0. && !m.is_nan()) {
                let new_price = if update.has_own("price") {
                    update.get("price")?
                } else {
                    order.get("price")?
                }
                .to_number()?;
                let sell = order.get("side")?.strict_equals(&Js::str("sell"));
                let take_profit = if sell {
                    new_price > mark
                } else {
                    new_price <= mark
                };
                let projected = Js::str(if take_profit { TAKE_PROFIT } else { STOP_LOSS });
                cancel(&rt, &order).await?;
                let replacement = merge(&order, &update)?;
                replacement.set("type", projected.clone())?;
                let ids = if primary {
                    place_stop_loss_or_take_profit(&rt, &replacement).await?
                } else {
                    await_js(
                        rt.this
                            .call_method("placeStopLossOrTakeProfit", vec![replacement])?,
                    )
                    .await?
                };
                let local = update.spread_object()?;
                local.set("type", projected)?;
                apply_local(&rt, &order, local, primary)?;
                return Ok(ids);
            }
        }

        let mut updated = vec![];
        let replace = endpoint("REPLACE_ORDER");

        if update.has_own("price") && is_virtual {
            let parent_id = id
                .replacen("__stop_loss", "", 1)
                .replacen("__take_profit", "", 1);
            let parent = venue(&rt)?
                .base
                .store
                .orders()
                .iter()
                .find(|o| o.id == parent_id)
                .map(|o| o.id.clone());
            if let Some(parent_id) = parent {
                let key = if id.ends_with("__stop_loss") {
                    "stopLoss"
                } else {
                    "takeProfit"
                };
                let payload = Js::object(vec![
                    ("category", category(&rt, &symbol)?),
                    ("orderId", Js::str(&parent_id)),
                    ("symbol", Js::str(&symbol)),
                    (key, template(&update.get("price")?)?),
                ]);
                let attempt = async {
                    let data = post(&rt, replace, payload).await?;
                    let message = data.get("retMsg")?;
                    if message.strict_equals(&Js::str("OK")) {
                        updated.push(result_order_id(&data)?);
                        apply_local(&rt, &order, update.clone(), primary)?;
                    } else if !message.strict_equals(&Js::str("order not modified")) {
                        emit(&rt, vec![message.clone()])?;
                        return Err(error(&message)?);
                    }
                    Ok(())
                }
                .await;
                if let Err(err) = attempt {
                    let response = err.value.get("response").unwrap_or(Js::Undefined);
                    let data = if response.is_nullish() {
                        Js::Undefined
                    } else {
                        response.get("data")?
                    };
                    let message = if data.is_nullish() {
                        Js::Undefined
                    } else {
                        data.get("retMsg")?
                    };
                    let message = if message.truthy() {
                        message
                    } else {
                        err.value.get("message")?
                    };
                    emit(&rt, vec![err.value.clone(), message])?;
                    return Err(err);
                }
            }
        }

        let is_conditional = !is_virtual && (is_stop || kind == "conditional") && !position_tpsl;

        if kind == "limit" || is_conditional {
            let payload = Js::object(vec![
                ("category", category(&rt, &symbol)?),
                ("orderId", Js::str(&id)),
                ("symbol", Js::str(&symbol)),
            ]);
            if update.has_own("amount") {
                payload.set("qty", template(&update.get("amount")?)?)?;
            }
            if update.has_own("price") {
                let key = if is_conditional {
                    "triggerPrice"
                } else {
                    "price"
                };
                payload.set(key, template(&update.get("price")?)?)?;
            }
            let data = post(&rt, replace, payload).await?;
            let message = data.get("retMsg")?;
            if message.strict_equals(&Js::str("OK")) {
                updated.push(result_order_id(&data)?);
                apply_local(&rt, &order, update.clone(), primary)?;
            } else if !message.strict_equals(&Js::str("order not modified")) {
                emit(&rt, vec![message])?;
            }
        }

        if !is_conditional && !is_virtual && is_stop {
            let amount = order.get("amount")?;
            let partial = !amount.strict_equals(&Js::Number(0.));
            let position_index = if primary {
                let order_json = to_json(&order)?.as_object().cloned().unwrap_or_default();
                get_order_position_idx(&rt, order_json).await?
            } else {
                await_js(
                    rt.this
                        .call_method("getOrderPositionIdx", vec![order.clone()])?,
                )
                .await?
                .to_number()?
            };
            let payload = Js::object(vec![
                ("category", category(&rt, &symbol)?),
                ("symbol", Js::str(&symbol)),
                ("positionIdx", Js::Number(position_index)),
                (
                    "tpslMode",
                    if primary {
                        Js::str(if partial { "Partial" } else { "Full" })
                    } else {
                        Js::Undefined
                    },
                ),
            ]);
            if !primary {
                payload.delete(&"tpslMode".encode_utf16().collect::<Vec<_>>());
            }
            let stop_loss = kind == STOP_LOSS;
            if update.has_own("price") {
                let key = if stop_loss { "stopLoss" } else { "takeProfit" };
                payload.set(key, template(&update.get("price")?)?)?;
            }
            if primary && partial {
                let size = if update.has_own("amount") {
                    update.get("amount")?
                } else {
                    amount
                };
                let key = if stop_loss { "slSize" } else { "tpSize" };
                payload.set(key, template(&size)?)?;
            }
            let data = post(&rt, endpoint("SET_TRADING_STOP"), payload).await?;
            let message = data.get("retMsg")?;
            if message.strict_equals(&Js::str("OK")) {
                apply_local(&rt, &order, update, primary)?;
            } else if !message.strict_equals(&Js::str("order not modified")) {
                emit(&rt, vec![message])?;
            }
        }

        Ok(Js::array(updated))
    })))
}
