//! Position controls: leverage, position mode and margin mode over position_controls.
use crate::{
    binding::{
        host::text,
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, from_json, to_json},
    },
    position_controls::{self as controls, Positions},
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
        let response = await_js(pending).await.map_err(to_error)?;
        Ok(Rc::new(
            to_json(&response.get("data").map_err(to_error)?).map_err(to_error)?,
        ))
    })
}

/// `this.fetchPositions(symbols)` awaited, as the retained JSON batch.
fn fetch(
    rt: &Rt,
    symbols: Option<Vec<String>>,
) -> Result<impl Future<Output = Result<Positions, Error>> + use<>, Error> {
    let symbols = symbols.map_or(Js::Undefined, |s| {
        Js::array(s.iter().map(|s| Js::str(s)).collect())
    });
    let pending = rt
        .this
        .call_method("fetchPositions", vec![symbols])
        .map_err(to_error)?;
    Ok(async move {
        let positions = await_js(pending).await.map_err(to_error)?;
        match to_json(&positions).map_err(to_error)? {
            Value::Array(rows) => Ok(Rc::new(rows)),
            _ => Ok(Rc::new(vec![])),
        }
    })
}

fn store_call(rt: &Rt, member: &str, value: Js) -> Result<(), Error> {
    rt.this
        .get("store")
        .and_then(|store| store.call_method(member, vec![value]))
        .map_err(to_error)?;
    Ok(())
}

fn positions_js(positions: &Positions) -> Js {
    Js::array(positions.iter().map(from_json).collect())
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

// ts: BybitExchange.setLeverage
pub(crate) fn set_leverage(rt: &Rt, symbol: Js, leverage: Js) -> Result<Js, Thrown> {
    let symbol = text(&symbol)?;
    let leverage = leverage.to_number()?;
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let venue = venue(&rt)?;
        controls::set_leverage(
            &symbol,
            leverage,
            |symbol| {
                Ok(venue
                    .base
                    .store
                    .markets()
                    .iter()
                    .find(|m| m.symbol == symbol)
                    .map(|m| (m.limits.leverage.min, m.limits.leverage.max)))
            },
            || {
                let positions = serde_json::to_value(venue.base.store.positions().to_vec())
                    .map_err(|e| to_error(Thrown::error("TypeError", &e.to_string())))?;
                Ok(Rc::new(positions.as_array().cloned().unwrap_or_default()))
            },
            |symbol| Ok(venue.get_category_for_symbol(symbol).into()),
            |endpoint, payload| post(&rt, endpoint, payload),
            |symbols| fetch(&rt, symbols),
            |positions| store_call(&rt, "addOrUpdatePositions", positions_js(&positions)),
            |message| emit(&rt, message),
        )
        .await
        .map_err(to_thrown)?;
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
            |positions| {
                store_call(
                    &rt,
                    "update",
                    Js::object(vec![("positions", positions_js(&positions))]),
                )
            },
            |positions| store_call(&rt, "addOrUpdatePositions", positions_js(&positions)),
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
                Ok(async move { await_js(pending).await.map(|_| ()).map_err(to_error) })
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
            |positions| {
                store_call(
                    &rt,
                    "update",
                    Js::object(vec![("positions", positions_js(&positions))]),
                )
            },
        )
        .await
        .map_err(to_thrown)?;
        Ok(Js::Undefined)
    })))
}
