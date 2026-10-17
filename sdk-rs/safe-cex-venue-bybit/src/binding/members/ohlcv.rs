//! Candle reads share the source tuple mapping and each class's response checks.
use crate::binding::{
    host::text,
    rt::{Rt, venue},
    value::{Js, Thrown},
};
use safe_cex_types::ExchangeName;

fn keys(rt: &Rt, value: &Js) -> Result<String, Thrown> {
    text(
        &rt.global("Object.keys")?
            .call(Js::Undefined, vec![value.clone()])?
            .call_method("join", vec![Js::str(",")])?,
    )
}

fn validate_primary_response(rt: &Rt, response: &Js, data: &Js) -> Result<(), Thrown> {
    if !data.truthy() {
        let status = text(&response.get("status")?)?;
        let adapter = rt.this.get("xhr")?.get("defaults")?.get("adapter")?;
        rt.shared.runtime.console(
            "error",
            &format!(
                "[BYBIT-KLINE] null data! response keys={} status={status} adapter={}",
                keys(rt, response)?,
                adapter.type_of()
            ),
        );
        return Err(Thrown::error(
            "Error",
            &format!("Bybit KLINE: null response data (status={status})"),
        ));
    }
    let code = data.get("retCode")?;
    if !code.is_undefined() && !code.strict_equals(&Js::Number(0.0)) {
        let message = data.get("retMsg")?;
        let message = if message.truthy() {
            text(&message)?
        } else {
            "unknown".into()
        };
        return Err(Thrown::error(
            "Error",
            &format!("Bybit KLINE error: {message} (retCode={})", text(&code)?),
        ));
    }
    let result = data.get("result")?;
    let list = if result.is_nullish() {
        Js::Undefined
    } else {
        result.get("list")?
    };
    if !list.truthy() {
        return Err(Thrown::error(
            "Error",
            &format!(
                "Bybit KLINE: no candle list in response (keys={})",
                keys(rt, data)?
            ),
        ));
    }
    Ok(())
}

pub(crate) fn fetch_ohlcv(rt: &Rt, options: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        let name = venue(&rt)?.base.name.clone();
        let primary = matches!(
            name,
            ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
        );
        if primary {
            rt.this
                .call_method("validateCandleInterval", vec![options.get("interval")?])?;
        }
        if name == ExchangeName::Bybitspot {
            return crate::spot_exchange_reads::fetch_ohlcv(&rt, &rt.this, options).await;
        }
        crate::spot_exchange_reads::fetch_ohlcv_for_category(
            &rt,
            &rt.this,
            options,
            |options| {
                rt.this
                    .call_method("getCategoryForSymbol", vec![options.get("symbol")?])
            },
            !primary,
            |response, data| {
                if primary {
                    validate_primary_response(&rt, response, data)
                } else {
                    Ok(())
                }
            },
        )
        .await
    })))
}
