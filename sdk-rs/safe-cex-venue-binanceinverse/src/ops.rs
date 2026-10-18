//! Native operations shared by the source-corresponding COIN-M class bodies.
use crate::Rt;
use safe_cex_exchange::js_runtime::{
    library::in_operator,
    promise::{JsFuture, await_js},
    scope::scope_of,
    value::{Js, Thrown},
};

pub(crate) fn contains(value: &Js, key: &str) -> Result<bool, Thrown> {
    in_operator(value, key)
}
pub(crate) fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
pub(crate) fn or(value: Js, fallback: impl FnOnce() -> Result<Js, Thrown>) -> Result<Js, Thrown> {
    if value.truthy() {
        Ok(value)
    } else {
        fallback()
    }
}
pub(crate) fn text(value: &Js) -> Result<String, Thrown> {
    String::from_utf16(&value.to_js_string()?)
        .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed string boundary"))
}
pub(crate) fn global(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    rt.global(name)?.call(Js::Undefined, args)
}
pub(crate) fn v(rt: &Rt, object: &Js, key: &str) -> Result<Js, Thrown> {
    global(rt, "v", vec![object.clone(), Js::str(key)])
}
pub(crate) fn parse(rt: &Rt, value: Js) -> Result<f64, Thrown> {
    global(rt, "parseFloat", vec![value])?.to_number()
}
pub(crate) fn parse_or_zero(rt: &Rt, value: Js) -> Result<f64, Thrown> {
    parse(rt, or(value, || Ok(Js::str("0")))?)
}
pub(crate) fn store(rt: &Rt, key: &str) -> Result<Js, Thrown> {
    rt.this.get("store")?.get(key)
}
pub(crate) fn error_message(error: &Js) -> Result<Js, Thrown> {
    or(
        optional(&optional(&optional(error, "response")?, "data")?, "msg")?,
        || optional(error, "message"),
    )
}
pub(crate) fn emit(rt: &Rt, error: &Js) -> Result<(), Thrown> {
    rt.call_path("emitAndLogError", vec![error_message(error)?])?;
    Ok(())
}
pub(crate) async fn request(
    rt: &Rt,
    client: &str,
    method: &str,
    endpoint: &str,
    extra: Vec<Js>,
) -> Result<Js, Thrown> {
    let path = rt.global("ENDPOINTS")?.get(endpoint)?;
    let mut args = vec![path];
    args.extend(extra);
    await_js(rt.this.get(client)?.call_method(method, args)?)
        .await?
        .get("data")
}
pub(crate) fn require_array(endpoint: &Js, data: Js) -> Result<Vec<Js>, Thrown> {
    if let Some(rows) = data.array_values() {
        return Ok(rows);
    }
    let shape = if data.is_nullish() && !data.is_undefined() {
        "null".to_owned()
    } else if data.type_of() == "object" {
        let names = data
            .own_keys()
            .into_iter()
            .take(6)
            .map(|key| Js::utf16(&key))
            .collect();
        let joined = Js::array(names).call_method("join", vec![Js::str(",")])?;
        if joined.to_js_string()?.is_empty() {
            "object".to_owned()
        } else {
            format!("object({})", text(&joined)?)
        }
    } else {
        data.type_of().to_owned()
    };
    let message = if data.truthy() && data.type_of() == "object" {
        let message = or(data.get("msg")?, || {
            or(data.get("message")?, || data.get("error"))
        })?;
        let code = data.get("code")?;
        let mut parts = vec![];
        if !code.is_undefined() {
            parts.push(format!("code={}", text(&code)?));
        }
        if message.truthy() {
            parts.push(text(&message)?);
        }
        parts.join(" ")
    } else {
        String::new()
    };
    let suffix = if message.is_empty() {
        String::new()
    } else {
        format!(" ({message})")
    };
    Err(Thrown::error(
        "Error",
        &format!(
            "Binance COIN-M {} response malformed: expected array, received {shape}{suffix}",
            text(endpoint)?
        ),
    ))
}
pub(crate) fn enum_value(rt: &Rt, family: &str, key: &str) -> Result<Js, Thrown> {
    rt.global(family)?.get(key)
}
pub(crate) fn install_async(
    rt: &Rt,
    name: &'static str,
    body: fn(Rt, Vec<Js>) -> JsFuture<'static>,
) -> Result<(), Thrown> {
    let weak = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        name,
        Js::function(name, move |_, args| {
            let rt = scope_of(&weak)?;
            Ok(rt.spawn(body(rt.clone(), args)))
        }),
    )
}
pub(crate) fn install_sync(
    rt: &Rt,
    name: &'static str,
    body: fn(&Rt, &[Js]) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let weak = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        name,
        Js::function(name, move |_, args| body(&scope_of(&weak)?, &args)),
    )
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    crate::reads::install(rt)?;
    crate::lifecycle::install(rt)?;
    crate::orders::install(rt)?;
    Ok(())
}
