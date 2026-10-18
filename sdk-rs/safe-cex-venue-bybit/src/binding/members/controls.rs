//! Position controls: leverage, position mode and margin mode over position_controls.
use crate::{
    binding::{
        host::{get_key, text},
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, from_json, to_json},
    },
    position_controls as controls,
};
use serde_json::{Map, Value};
use std::rc::Rc;

type Error = Rc<Value>;

fn to_error(error: Thrown) -> Error {
    Rc::new(to_json(&error.value).unwrap_or(Value::Null))
}

fn to_thrown(error: Error) -> Thrown {
    Thrown::new(from_json(&error))
}

fn optional_text(value: &Js) -> Result<Option<String>, Thrown> {
    if value.is_nullish() {
        Ok(None)
    } else {
        Some(text(value)).transpose()
    }
}

/// `(await this.xhr.post(endpoint, payload)).data`.
fn post(
    rt: &Rt,
    endpoint: &str,
    payload: Map<String, Value>,
) -> Result<impl Future<Output = Result<Rc<Value>, Error>> + use<>, Error> {
    let xhr = rt.this.get("xhr").map_err(to_error)?;
    let pending = xhr
        .call_method(
            "post",
            vec![Js::str(endpoint), from_json(&Value::Object(payload))],
        )
        .map_err(to_error)?;
    Ok(async move {
        let response = await_js(&rt.exchange_runtime(), pending).await.map_err(to_error)?;
        Ok(Rc::new(
            to_json(&response.get("data").map_err(to_error)?).map_err(to_error)?,
        ))
    })
}

/// Preserve the native fetched batch through Store publication.
fn fetch(
    rt: &Rt,
    symbols: Option<Vec<String>>,
) -> Result<impl Future<Output = Result<Js, Error>> + use<>, Error> {
    let symbols = symbols.map_or(Js::Undefined, |s| {
        Js::array(s.iter().map(|s| Js::str(s)).collect())
    });
    let pending = rt
        .this
        .call_method("fetchPositions", vec![symbols])
        .map_err(to_error)?;
    Ok(async move { await_js(&rt.exchange_runtime(), pending).await.map_err(to_error) })
}

fn store_call(rt: &Rt, member: &str, value: Js) -> Result<(), Error> {
    rt.this
        .get("store")
        .and_then(|store| store.call_method(member, vec![value]))
        .map_err(to_error)?;
    Ok(())
}

fn emit(rt: &Rt, message: Option<Value>) -> Result<(), Error> {
    rt.this
        .call_method(
            "emitAndLogError",
            vec![message.as_ref().map_or(Js::Undefined, from_json)],
        )
        .map_err(to_error)?;
    Ok(())
}

// ts: isBybitLeverageNotModified — retained numeric/text coercions, no JSON node.
fn leverage_not_modified(data: &Js) -> Result<bool, Thrown> {
    let code = get_key(data, "retCode")?.to_number()?;
    let message = get_key(data, "retMsg")?;
    let message = text(&if message.truthy() {
        message
    } else {
        Js::str("")
    })?
    .to_lowercase();
    Ok(code == 110043. || (message.contains("leverage") && message.contains("not modified")))
}

// ts: BybitExchange.setLeverage. Keep Store/fetched rows in Js throughout the
// comparison and publication; a current NaN must differ from every requested
// leverage, and a refreshed model can contain native nonfinite values.
pub(crate) fn set_leverage(rt: &Rt, symbol: Js, leverage: Js) -> Result<Js, Thrown> {
    set_leverage_body(rt, symbol, leverage, true)
}

// ts: BybitInverseExchange.setLeverage — its source has neither the option
// refusal nor the primary leverage-not-modified exemption.
pub(crate) fn set_inverse_leverage(rt: &Rt, symbol: Js, leverage: Js) -> Result<Js, Thrown> {
    set_leverage_body(rt, symbol, leverage, false)
}

fn set_leverage_body(rt: &Rt, symbol: Js, leverage: Js, primary: bool) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        let symbol_text = text(&symbol)?;
        if primary && crate::options::is_bybit_option_symbol(&symbol_text) {
            return Err(Thrown::error(
                "Error",
                &format!("Bybit has no leverage for options ({symbol_text})"),
            ));
        }
        let store = rt.this.get("store")?;
        let market = store
            .get("marketsBySymbol")?
            .get_key(&symbol.to_js_string()?)?;
        if !market.truthy() {
            return Err(Thrown::error(
                "Error",
                &format!("Market {} not found", text(&symbol)?),
            ));
        }
        let selected_symbol = symbol.clone();
        let mut positions = store.get("positions")?.call_method(
            "filter",
            vec![Js::function("leveragePositionSymbol", move |_, args| {
                Ok(Js::Bool(
                    crate::binding::value::arg(&args, 0)
                        .get("symbol")?
                        .strict_equals(&selected_symbol),
                ))
            })],
        )?;
        if positions.get("length")?.strict_equals(&Js::Number(0.)) {
            positions = await_js(&rt.exchange_runtime(), 
                rt.this
                    .call_method("fetchPositions", vec![Js::array(vec![symbol.clone()])])?,
            )
            .await?;
            store.call_method("addOrUpdatePositions", vec![positions.clone()])?;
        }
        let first = positions.get("0")?;
        let current = if first.is_nullish() {
            Js::Undefined
        } else {
            first.get("leverage")?
        };
        let current = if current.is_nullish() {
            Js::Number(1.)
        } else {
            current
        };
        let limits = market.get("limits")?.get("leverage")?;
        let next = safe_cex_num::math_min(
            safe_cex_num::math_max(leverage.to_number()?, limits.get("min")?.to_number()?),
            limits.get("max")?.to_number()?,
        );
        if !current.strict_equals(&Js::Number(next)) {
            let data = await_js(&rt.exchange_runtime(), rt.this.get("xhr")?.call_method(
                "post",
                vec![
                    Js::str(crate::types::ENDPOINTS["SET_LEVERAGE"].as_str().unwrap()),
                    Js::object(vec![
                            (
                                "category",
                                rt.this
                                    .call_method("getCategoryForSymbol", vec![symbol.clone()])?,
                            ),
                            ("symbol", symbol.clone()),
                            (
                                "buyLeverage",
                                Js::str(&safe_cex_num::js_number_string(next)),
                            ),
                            (
                                "sellLeverage",
                                Js::str(&safe_cex_num::js_number_string(next)),
                            ),
                        ]),
                ],
            )?)
            .await?
            .get("data")?;
            if !get_key(&data, "retMsg")?.strict_equals(&Js::str("OK"))
                && !(primary && leverage_not_modified(&data)?)
            {
                let message = get_key(&data, "retMsg")?;
                rt.this
                    .call_method("emitAndLogError", vec![message.clone()])?;
                return Err(Thrown::error(
                    "Error",
                    &if message.is_undefined() {
                        String::new()
                    } else {
                        text(&message)?
                    },
                ));
            }
            let positions = await_js(&rt.exchange_runtime(), 
                rt.this
                    .call_method("fetchPositions", vec![Js::array(vec![symbol])])?,
            )
            .await?;
            store.call_method("addOrUpdatePositions", vec![positions])?;
        }
        Ok(Js::Undefined)
    })))
}

// ts: BybitExchange.changePositionModeHelper
pub(crate) fn change_position_mode_helper(rt: &Rt, args: &[Js]) -> Result<Js, Thrown> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Js::Undefined);
    let hedged = arg(0).truthy();
    let (category, coin, symbol) = (
        optional_text(&arg(1))?,
        optional_text(&arg(2))?,
        optional_text(&arg(3))?,
    );
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        controls::change_position_mode_helper(
            hedged,
            category.as_deref(),
            coin.as_deref(),
            symbol.as_deref(),
            |endpoint, payload| post(&rt, endpoint, payload),
            |symbols| fetch(&rt, symbols),
            |positions| store_call(&rt, "update", Js::object(vec![("positions", positions)])),
            |positions| store_call(&rt, "addOrUpdatePositions", positions),
            |message| emit(&rt, message),
        )
        .await
        .map_err(to_thrown)?;
        Ok(Js::Undefined)
    })))
}

// ts: BybitExchange.changePositionMode
pub(crate) fn change_position_mode(rt: &Rt, hedged: Js, symbol: Js) -> Result<Js, Thrown> {
    let hedged = hedged.truthy();
    let symbol = optional_text(&symbol)?;
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let this = body_rt.this.clone();
        controls::change_primary_position_mode(
            hedged,
            symbol.as_deref(),
            |hedged, category, coin| {
                let pending = this
                    .call_method(
                        "changePositionModeHelper",
                        vec![Js::Bool(hedged), Js::str(category), Js::str(coin)],
                    )
                    .map_err(to_error)?;
                Ok(async move { await_js(&rt.exchange_runtime(), pending).await.map(|_| ()).map_err(to_error) })
            },
        )
        .await
        .map_err(to_thrown)?;
        Ok(Js::Undefined)
    })))
}

// ts: BybitExchange.changeMarginType
pub(crate) fn change_margin_type(rt: &Rt, isolated: Js, symbol: Js) -> Result<Js, Thrown> {
    let isolated = isolated.truthy();
    let symbol = optional_text(&symbol)?;
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let venue = venue(&rt)?;
        controls::change_margin_type(
            isolated,
            symbol.as_deref(),
            |endpoint, payload| post(&rt, endpoint, payload),
            |message| emit(&rt, message),
            |margin_type| {
                venue.margin_type.set(margin_type);
                Ok(())
            },
            |symbols| fetch(&rt, symbols),
            |positions| store_call(&rt, "update", Js::object(vec![("positions", positions)])),
        )
        .await
        .map_err(to_thrown)?;
        Ok(Js::Undefined)
    })))
}
