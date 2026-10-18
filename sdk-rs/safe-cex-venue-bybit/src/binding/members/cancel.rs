//! Per-category batch cancels and venue-side cancellation of every order for a symbol.
use super::lifecycle::{mark_order_seen, retire_cancelled_orders};
use crate::{
    binding::{
        host::text,
        promise::await_js,
        rt::Rt,
        value::{Js, Thrown, error_object, to_json},
    },
    exchange_helpers::is_bybit_order_already_gone_cancel_message,
};

fn already_gone(message: &Js) -> Result<bool, Thrown> {
    is_bybit_order_already_gone_cancel_message(Some(&to_json(message)?))
        .map_err(|m| Thrown::error("TypeError", &m))
}

// ts: BybitExchange.cancelSymbolOrders / BybitInverseExchange.cancelSymbolOrders
pub(crate) fn cancel_symbol_orders(rt: &Rt, symbol: Js, cancel_tp_sl: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        let cancel_tp_sl = cancel_tp_sl.is_undefined() || cancel_tp_sl.truthy();
        let orders = rt
            .this
            .get("store")?
            .get("orders")?
            .array_values()?
            .unwrap_or_default();
        let mut tp_sl = vec![];
        let mut cancelled = vec![];
        for order in orders {
            if !order.get("symbol")?.strict_equals(&symbol) {
                continue;
            }
            if !cancel_tp_sl && !order.get("type")?.strict_equals(&Js::str("limit")) {
                tp_sl.push(order.clone());
            }
            cancelled.push(Js::object(vec![
                ("id", order.get("id")?),
                ("symbol", order.get("symbol")?),
            ]));
        }
        let category = rt
            .this
            .call_method("getCategoryForSymbol", vec![symbol.clone()])?;
        let payload = Js::object(vec![("category", category), ("symbol", symbol)]);
        let endpoint = crate::types::ENDPOINTS["CANCEL_SYMBOL_ORDERS"]
            .as_str()
            .unwrap_or_default();
        let data = await_js(
            &rt.exchange_runtime(),
            rt.this
                .get("unlimitedXHR")?
                .call_method("post", vec![Js::str(endpoint), payload])?,
        )
        .await?
        .get("data")?;
        if !tp_sl.is_empty() {
            let mut replacements = vec![];
            for order in tp_sl {
                let replacement = order.spread_object()?;
                let side = if order.get("side")?.strict_equals(&Js::str("buy")) {
                    "sell"
                } else {
                    "buy"
                };
                replacement.set("side", Js::str(side))?;
                replacements.push(replacement);
            }
            await_js(
                &rt.exchange_runtime(),
                rt.this
                    .call_method("placeOrders", vec![Js::array(replacements)])?,
            )
            .await?;
        }
        let message = crate::binding::host::get_key(&data, "retMsg")?;
        if !message.strict_equals(&Js::str("OK")) {
            rt.this.call_method("emitAndLogError", vec![message])?;
            return Ok(Js::Undefined);
        }
        rt.this
            .call_method("retireCancelledOrders", vec![Js::array(cancelled)])?;
        Ok(Js::Undefined)
    })))
}

fn fail(rt: &Rt, message: Js) -> Result<Thrown, Thrown> {
    rt.this
        .call_method("emitAndLogError", vec![message.clone()])?;
    let message = if message.is_undefined() {
        String::new()
    } else {
        text(&message)?
    };
    Ok(Thrown::new(error_object("Error", &message)))
}

// ts: BybitExchange.cancelOrdersHelper
pub(crate) async fn cancel_orders_helper(
    rt: &Rt,
    orders: Vec<Js>,
    category: &str,
) -> Result<(), Thrown> {
    let endpoint = crate::types::ENDPOINTS["CANCEL_ORDERS"]
        .as_str()
        .unwrap_or_default();
    for (i, lot) in orders.chunks(10).enumerate() {
        if i > 0 {
            let sleep = rt
                .global("sleep")?
                .call(Js::Undefined, vec![Js::Number(1000.)])?;
            await_js(&rt.exchange_runtime(), sleep).await?;
        }
        let mut request = Vec::with_capacity(lot.len());
        for order in lot {
            request.push(Js::object(vec![
                ("symbol", order.get("symbol")?),
                ("orderId", order.get("id")?),
            ]));
        }
        let payload = Js::object(vec![
            ("category", Js::str(category)),
            ("request", Js::array(request)),
        ]);
        let xhr = rt.this.get("unlimitedXHR")?;
        let data = await_js(
            &rt.exchange_runtime(),
            xhr.call_method("post", vec![Js::str(endpoint), payload])?,
        )
        .await?
        .get("data")?;
        let lot = Js::array(lot.to_vec());
        let message = data.get("retMsg")?;
        if !message.strict_equals(&Js::str("OK")) {
            if already_gone(&message)? {
                retire_cancelled_orders(rt, &lot)?;
                continue;
            }
            return Err(fail(rt, message)?);
        }
        // `data.retExtInfo?.list.find(...)`: an absent retExtInfo short-circuits to no error.
        let ext = data.get("retExtInfo")?;
        let list = if ext.is_nullish() {
            Js::array(vec![])
        } else {
            ext.get("list")?
        };
        let Some(rows) = list.array_values()? else {
            return Err(Thrown::type_error(&format!(
                "Cannot read properties of {} (reading 'find')",
                list.to_display()
            )));
        };
        for row in rows {
            let code = row.get("code")?;
            let message = row.get("msg")?;
            if !code.strict_equals(&Js::Number(0.)) && !already_gone(&message)? {
                return Err(fail(rt, message)?);
            }
        }
        retire_cancelled_orders(rt, &lot)?;
    }
    Ok(())
}

// ts: BybitExchange.cancelOrders
pub(crate) fn cancel_orders(rt: &Rt, orders: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let orders = orders.array_values()?.unwrap_or_default();
        let mut kept = vec![];
        for order in &orders {
            let id = text(&order.get("id")?)?;
            mark_order_seen(&rt, &id)?;
        }
        for order in orders {
            let id = text(&order.get("id")?)?;
            if !id.ends_with("__stop_loss") && !id.ends_with("__take_profit") {
                kept.push(order);
            }
        }
        if kept.is_empty() {
            return Ok(Js::Undefined);
        }
        let mut by_category: Vec<(String, Vec<Js>)> = vec![];
        for order in kept {
            let category = text(
                &rt.this
                    .call_method("getCategoryForSymbol", vec![order.get("symbol")?])?,
            )?;
            match by_category.iter_mut().find(|(c, _)| *c == category) {
                Some((_, list)) => list.push(order),
                None => by_category.push((category, vec![order])),
            }
        }
        for (category, orders) in by_category {
            cancel_orders_helper(&rt, orders, &category).await?;
        }
        Ok(Js::Undefined)
    })))
}
