//! The dedicated Inverse mode and margin writes retain native Position rows.
use crate::binding::{
    host::{get_key, text},
    promise::await_js,
    rt::Rt,
    value::{Js, Thrown, arg},
};

fn method(
    rt: &Rt,
    name: &'static str,
    body: fn(Rt, Vec<Js>) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let rt = rt.clone();
    rt.this
        .clone()
        .define(name, Js::function(name, move |_, a| body(rt.clone(), a)))
}
fn error(message: &Js) -> Result<Thrown, Thrown> {
    Ok(Thrown::error(
        "Error",
        &if message.is_undefined() {
            String::new()
        } else {
            text(message)?
        },
    ))
}
async fn post(rt: &Rt, name: &str, request: Js) -> Result<Js, Thrown> {
    await_js(
        &rt.exchange_runtime(),
        rt.this
            .get("xhr")?
            .call_method("post", vec![rt.global("ENDPOINTS")?.get(name)?, request])?,
    )
    .await?
    .get("data")
}
fn emit(rt: &Rt, message: Js) -> Result<(), Thrown> {
    rt.this.call_method("emitAndLogError", vec![message])?;
    Ok(())
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    method(rt, "changePositionModeHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let symbol=arg(&a,3);
            let data=post(&rt,"SET_POSITION_MODE",Js::object(vec![
                ("category",arg(&a,1)),("coin",arg(&a,2)),("symbol",symbol.clone()),
                ("mode",Js::Number(if arg(&a,0).truthy(){3.}else{0.})),
            ])).await?;
            let positions=await_js(&rt.exchange_runtime(), rt.this.call_method("fetchPositions",vec![])?).await?;
            let store=rt.this.get("store")?;
            store.call_method("update",vec![Js::object(vec![("positions",positions)])])?;
            if symbol.truthy() {
                let positions=await_js(&rt.exchange_runtime(), rt.this.call_method("fetchPositions",vec![Js::array(vec![symbol])])?).await?;
                store.call_method("addOrUpdatePositions",vec![positions])?;
            }
            let message=data.get("retMsg")?;
            if !message.strict_equals(&Js::str("All symbols switched successfully.")) {
                emit(&rt,message.clone())?;
                if !message.strict_equals(&Js::str("Partial symbols switched successfully, excluding symbols with open orders or positions.")) {
                    return Err(error(&message)?);
                }
            }
            Ok(Js::Undefined)
        })))
    })?;
    method(rt, "changeMarginType", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let isolated = arg(&a, 0).truthy();
            let data = post(
                &rt,
                "SET_MARGIN_MODE",
                Js::object(vec![(
                    "setMarginMode",
                    Js::str(if isolated {
                        "ISOLATED_MARGIN"
                    } else {
                        "REGULAR_MARGIN"
                    }),
                )]),
            )
            .await?;
            let result = if data.is_nullish() {
                Js::Undefined
            } else {
                data.get("result")?
            };
            let reasons = if result.is_nullish() {
                Js::Undefined
            } else {
                result.get("reasons")?
            };
            let reasons = if reasons.is_array() {
                reasons
            } else {
                Js::array(vec![])
            };
            if get_key(&data, "retCode")?.to_number()? != 0. || reasons.get("length")?.truthy() {
                let message = reasons
                    .call_method(
                        "map",
                        vec![Js::function("inverseMarginReason", |_, a| {
                            let reason = arg(&a, 0);
                            if reason.is_nullish() {
                                Ok(Js::Undefined)
                            } else {
                                reason.get("reasonMsg")
                            }
                        })],
                    )?
                    .call_method(
                        "filter",
                        vec![Js::function("inverseTruthyReason", |_, a| {
                            Ok(Js::Bool(arg(&a, 0).truthy()))
                        })],
                    )?
                    .call_method("join", vec![Js::str(", ")])?;
                let message = if message.truthy() {
                    message
                } else {
                    let message = get_key(&data, "retMsg")?;
                    if message.truthy() {
                        message
                    } else {
                        Js::str("Failed to switch margin type")
                    }
                };
                emit(&rt, message.clone())?;
                return Err(error(&message)?);
            }
            rt.this.set(
                "marginType",
                Js::str(if isolated { "isolated" } else { "cross" }),
            )?;
            let positions = await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("fetchPositions", vec![])?,
            )
            .await?;
            rt.this
                .get("store")?
                .call_method("update", vec![Js::object(vec![("positions", positions)])])?;
            Ok(Js::Undefined)
        })))
    })?;
    Ok(())
}
