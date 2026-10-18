//! Execution history and order-history recovery on the actual Bybit receiver.
use crate::binding::{
    host::text,
    promise::await_js,
    rt::{Rt, venue},
    value::{Js, Thrown},
};
use safe_cex_types::ExchangeName;
use std::{cell::Cell, future::Future, pin::Pin};

type Read<'a> = Pin<Box<dyn Future<Output = Result<(), Thrown>> + 'a>>;
fn optional(owner: &Js, key: &str) -> Result<Js, Thrown> {
    if owner.is_nullish() {
        Ok(Js::Undefined)
    } else {
        owner.get(key)
    }
}
fn result_field(data: &Js, key: &str) -> Result<Js, Thrown> {
    optional(&optional(data, "result")?, key)
}
fn list(data: &Js) -> Result<Js, Thrown> {
    let rows = result_field(data, "list")?;
    Ok(if rows.is_array() {
        rows
    } else {
        Js::array(vec![])
    })
}
fn effective_limit(opts: &Js) -> Result<Js, Thrown> {
    let value = opts.get("limit")?;
    Ok(if value.truthy() {
        value
    } else {
        Js::Number(50.)
    })
}
fn log(rt: &Rt, message: &str) -> Result<(), Thrown> {
    rt.this
        .call_method("log", vec![Js::str(message), Js::str("warning")])?;
    Ok(())
}
fn console_warn(rt: &Rt, message: &str) -> Result<(), Thrown> {
    let console = rt.shared.globals.get("console")?;
    if !console.is_undefined() && console.get("warn")?.is_callable() {
        console.call_method("warn", vec![Js::str(message)])?;
    }
    Ok(())
}
fn params(rt: &Rt, opts: &Js, window: &Js, cursor: Js) -> Result<Js, Thrown> {
    let symbol = opts.get("symbol")?;
    let category = rt
        .this
        .call_method("getCategoryForSymbol", vec![symbol.clone()])?;
    let params = Js::object(vec![
        ("category", category),
        ("symbol", symbol),
        (
            "limit",
            Js::Number(safe_cex_num::math_min(
                effective_limit(opts)?.to_number()?,
                100.,
            )),
        ),
    ]);
    let order_id = opts.get("orderId")?;
    if order_id.truthy() {
        params.define("orderId", order_id)?;
    }
    for (source, target) in [("startTime", "startTime"), ("endTime", "endTime")] {
        let value = window.get(source)?;
        if !value.is_undefined() {
            params.define(target, value)?;
        }
    }
    if cursor.truthy() {
        params.define("cursor", cursor)?;
    }
    Ok(params)
}
async fn get(rt: &Rt, endpoint: &str, params: Js) -> Result<Js, Thrown> {
    let request = rt.this.get("xhr")?.call_method(
        "get",
        vec![Js::str(endpoint), Js::object(vec![("params", params)])],
    )?;
    await_js(&rt.exchange_runtime(), request).await?.get("data")
}
fn ret_msg(rt: &Rt, data: &Js) -> Result<String, Thrown> {
    let value = rt
        .global("v")?
        .call(Js::Undefined, vec![data.clone(), Js::str("retMsg")])?;
    text(&if value.truthy() {
        value
    } else {
        Js::str("UNKNOWN")
    })
}
fn primary_page<'a>(
    rt: &'a Rt,
    opts: &'a Js,
    window: Js,
    cursor: Js,
    depth: u32,
    executions: &'a Js,
    count: &'a Cell<f64>,
    inverse: bool,
) -> Read<'a> {
    Box::pin(async move {
        if depth >= 5 {
            return log(rt, "fetchHistoricalFills reached max recursion depth of 5");
        }
        let params = params(rt, opts, &window, cursor)?;
        let result = async {
            let data = get(rt, "/v5/execution/list", params).await?;
            let message = ret_msg(rt, &data)?;
            if message != "OK" {
                log(rt, &format!("Failed to fetch executions: {message}"))?;
                return Ok(());
            }
            let rows = list(&data)?;
            count.set(count.get() + rows.get("length")?.to_number()?);
            crate::primary_fills::append_primary_fills(
                &rows,
                &opts.get("symbol")?,
                executions,
                inverse,
            )?;
            let cursor = result_field(&data, "nextPageCursor")?;
            if cursor.truthy()
                && executions.get("length")?.to_number()? < effective_limit(opts)?.to_number()?
            {
                primary_page(
                    rt,
                    opts,
                    window,
                    cursor,
                    depth + 1,
                    executions,
                    count,
                    inverse,
                )
                .await?;
            }
            Ok::<(), Thrown>(())
        }
        .await;
        if let Err(error) = result {
            log(
                rt,
                &format!(
                    "Error fetching executions: {}",
                    text(&error.value.get("message")?)?
                ),
            )?;
        }
        Ok(())
    })
}
fn fallback_page<'a>(
    rt: &'a Rt,
    opts: &'a Js,
    window: Js,
    cursor: Js,
    depth: u32,
    executions: &'a Js,
) -> Read<'a> {
    Box::pin(async move {
        if depth >= 5 {
            return Ok(());
        }
        let params = params(rt, opts, &window, cursor)?;
        let data = get(rt, "/v5/order/history", params).await?;
        let message = ret_msg(rt, &data)?;
        if message != "OK" {
            return log(
                rt,
                &format!("Failed order-history fallback fetch executions: {message}"),
            );
        }
        let mapped = crate::history_mapping::map_order_history_rows_to_executions(
            &rt.shared.globals,
            &list(&data)?,
            opts.get("symbol")?,
            opts.get("orderId")?,
            opts.get("from")?,
            opts.get("to")?,
        )?;
        executions.call_method("push", mapped.iterable_values()?)?;
        let cursor = result_field(&data, "nextPageCursor")?;
        if cursor.truthy()
            && executions.get("length")?.to_number()? < effective_limit(opts)?.to_number()?
        {
            fallback_page(rt, opts, window, cursor, depth + 1, executions).await?;
        }
        Ok(())
    })
}
pub(crate) async fn fetch_executions_fallback_from_order_history(
    rt: &Rt,
    opts: Js,
    lookback: Js,
) -> Result<Js, Thrown> {
    let symbol = opts.get("symbol")?;
    if !symbol.truthy() {
        return Ok(Js::array(vec![]));
    }
    let to = opts.get("to")?;
    let fallback_to = if to.is_nullish() {
        rt.global("virtualClock.getCurrentTimeMs")?
            .call(Js::Undefined, vec![])?
    } else {
        to.clone()
    };
    let from = opts.get("from")?;
    let from = if from.is_nullish() {
        if to.is_undefined() {
            let lookback =
                if lookback.is_undefined() || venue(rt)?.base.name == ExchangeName::Bybitinverse {
                    rt.this.get("DEFAULT_EXECUTIONS_LOOKBACK_MS")?
                } else {
                    lookback
                };
            Js::Number(fallback_to.to_number()? - lookback.to_number()?)
        } else {
            Js::Undefined
        }
    } else {
        from
    };
    let executions = Js::array(vec![]);
    let windows = rt
        .this
        .call_method("getExecutionQueryWindows", vec![from, fallback_to])?;
    for window in windows.iterable_values()? {
        if executions.get("length")?.to_number()? >= effective_limit(&opts)?.to_number()? {
            break;
        }
        fallback_page(rt, &opts, window, Js::Undefined, 0, &executions).await?;
    }
    let mut rows = executions.array_values()?.unwrap();
    rows.sort_by(|a, b| {
        b.get("timestamp")
            .unwrap()
            .to_number()
            .unwrap()
            .total_cmp(&a.get("timestamp").unwrap().to_number().unwrap())
    });
    Js::array(rows).call_method("slice", vec![Js::Number(0.), effective_limit(&opts)?])
}
pub(crate) fn fetch_historical_fills(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        if rt.this.get("isPublicDataOnly")?.truthy() { return Ok(Js::array(vec![])); }
        let symbol = opts.get("symbol")?;
        if !symbol.truthy() { return Ok(Js::array(vec![])); }
        let order_id = opts.get("orderId")?;
        let from = opts.get("from")?;
        let to = opts.get("to")?;
        let limit = opts.get("limit")?;
        let limit = if limit.is_undefined() { Js::Number(50.) } else { limit };
        let inverse = venue(&rt)?.base.name == ExchangeName::Bybitinverse;
        let executions = Js::array(vec![]);
        let count = Cell::new(0.);
        let windows = rt.this.call_method("getExecutionQueryWindows", vec![from.clone(), to.clone()])?;
        for window in windows.iterable_values()? {
            if executions.get("length")?.to_number()? >= effective_limit(&opts)?.to_number()? { break; }
            primary_page(&rt, &opts, window, Js::Undefined, 0, &executions, &count, inverse).await?;
        }
        if executions.get("length")?.to_number()? > 0. { return Ok(executions); }
        let result = async {
            let warning = format!("fetchHistoricalFills primary path produced no valid fills for {}; falling back to /v5/order/history (rawRows={}, windows={}, limit={}, hasOrderId={})", text(&symbol)?, safe_cex_num::js_number_string(count.get()), text(&windows.get("length")?)?, text(&limit)?, order_id.truthy());
            log(&rt, &warning)?;
            let name = text(&rt.this.get("name")?)?;
            console_warn(&rt, &format!("[{name}] {warning}"))?;
            let fallback = await_js(&rt.exchange_runtime(), rt.this.call_method("fetchExecutionsFallbackFromOrderHistory", vec![opts.clone()])?).await?;
            if !inverse && (fallback.get("length")?.to_number()? > 0. || !from.is_undefined() || order_id.truthy()) { return Ok(fallback); }
            if !inverse && to.is_undefined() && rt.this.get("DEFAULT_EXECUTIONS_LOOKBACK_MS")?.to_number()? < 2592000000. {
                let warning = format!("fetchHistoricalFills short fallback returned no rows for {}; retrying 30d order-history lookup for recovery", text(&symbol)?);
                log(&rt, &warning)?;
                console_warn(&rt, &format!("[{name}] {warning}"))?;
                return await_js(&rt.exchange_runtime(), rt.this.call_method("fetchExecutionsFallbackFromOrderHistory", vec![opts.clone(), Js::Number(2592000000.)])?).await;
            }
            if fallback.get("length")?.to_number()? == 0. { log(&rt, &format!("fetchHistoricalFills fallback returned no rows for {}", text(&symbol)?))?; }
            Ok::<Js, Thrown>(fallback)
        }.await;
        match result {
            Ok(rows) => Ok(rows),
            Err(error) => {
                let message = optional(&error.value, "message")?;
                let message = if message.truthy() { message } else { error.value };
                log(&rt, &format!("Failed order-history fallback fetch executions: {}", text(&message)?))?;
                Ok(executions)
            }
        }
    })))
}
