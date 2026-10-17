//! Language operations over `Js`: receiver-relative name capture, the named
//! intrinsics the ported bodies call, and the per-receiver StreamOps frame.
use super::Shared;
use crate::public_streams::StreamOps;
use safe_cex_exchange::js_runtime::{
    promise::{Promise, await_js},
    scope::scope_of,
    value::{Js, Str, Thrown, arg},
};
use std::rc::{Rc, Weak};

pub(crate) type W = Weak<Shared>;

pub(crate) fn shared(weak: &W) -> Result<Rc<Shared>, Thrown> {
    weak.upgrade()
        .ok_or_else(|| Thrown::error("Error", "KuCoin runtime disposed"))
}

pub(crate) fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn units(v: &Js) -> Result<Str, Thrown> {
    v.to_js_string()
}

fn concat(parts: &[&[u16]]) -> Js {
    Js::String(parts.concat().into())
}

/// `${prefix}${a}${middle}${b}`-shaped template literal.
fn template(prefix: &str, values: &[&Js], separators: &[&str]) -> Result<Js, Thrown> {
    let mut out: Vec<u16> = prefix.encode_utf16().collect();
    for (i, v) in values.iter().enumerate() {
        out.extend_from_slice(&units(v)?);
        if let Some(sep) = separators.get(i) {
            out.extend(sep.encode_utf16());
        }
    }
    Ok(Js::String(out.into()))
}

fn is_string_like(v: &Js) -> bool {
    !matches!(
        v,
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::BigInt(_)
    )
}

/// Binary `+`: string concatenation when either primitive is a string.
pub(crate) fn js_add(a: &Js, b: &Js) -> Result<Js, Thrown> {
    if is_string_like(a) || is_string_like(b) {
        return Ok(concat(&[&units(a)?, &units(b)?]));
    }
    Ok(Js::Number(a.to_number()? + b.to_number()?))
}

/// Abstract relational `a < b`; strings compare by UTF-16 code units.
pub(crate) fn js_less(a: &Js, b: &Js) -> Result<bool, Thrown> {
    if let (Js::String(x), Js::String(y)) = (a, b) {
        return Ok(x < y);
    }
    Ok(a.to_number()? < b.to_number()?)
}

/// Abstract relational `a <= b` (false whenever either side is NaN).
pub(crate) fn js_less_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    if let (Js::String(x), Js::String(y)) = (a, b) {
        return Ok(x <= y);
    }
    Ok(a.to_number()? <= b.to_number()?)
}

/// `key in value`, walking own properties and the prototype chain.
pub(crate) fn has_property(key: &str, value: &Js) -> Result<bool, Thrown> {
    if !value.is_object() {
        return Err(Thrown::type_error(&format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            value.to_display()
        )));
    }
    let mut current = value.clone();
    loop {
        if current.has_own(key) {
            return Ok(true);
        }
        if matches!(current, Js::Array(_)) && key == "length" {
            return Ok(true);
        }
        let next = match &current {
            Js::Object(o) => o.proto.borrow().clone(),
            _ => None,
        };
        match next {
            Some(p) => current = p,
            None => return Ok(false),
        }
    }
}

fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

/// `value.member` as a bound callable.
pub(crate) use safe_cex_exchange::js_runtime::scope::bound_member as member;

/// Resolve `this.a.b` against `receiver` or a module-scope `A.b` from globals;
/// a callable last segment keeps its owner as receiver.
pub(crate) fn capture(weak: &W, receiver: &Js, path: &str) -> Result<Js, Thrown> {
    let mut parts = path.split('.');
    let first = parts.next().unwrap_or_default();
    let mut value = if first == "this" {
        receiver.clone()
    } else {
        let shared = shared(weak)?;
        if !shared.globals.has_own(first) {
            return Err(Thrown::error(
                "ReferenceError",
                &format!("{first} is not defined"),
            ));
        }
        shared.globals.get(first)?
    };
    let rest: Vec<&str> = parts.collect();
    for (i, key) in rest.iter().enumerate() {
        value = if i + 1 == rest.len() {
            member(&value, key)?
        } else {
            value.get(key)?
        };
    }
    Ok(value)
}

pub(crate) fn intrinsic(name: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let x = |i| arg(&a, i);
    let n = |i| x(i).to_number();
    let key = |i| -> Result<Str, Thrown> { units(&x(i)) };
    Ok(match name {
        "call-method" => {
            let receiver = x(0);
            let f = receiver.get_key(&key(1)?)?;
            f.call(receiver, a.iter().skip(2).cloned().collect())?
        }
        "call-with-this" => x(0).call(x(1), x(2).array_values().unwrap_or_default())?,
        "get-index" => x(0).get_key(&key(1)?)?,
        "set-index" => {
            x(0).set_key(&key(1)?, x(2))?;
            x(2)
        }
        "delete-index" => {
            x(0).delete(&key(1)?);
            Js::Bool(true)
        }
        "define-field" => {
            x(0).define_key(&key(1)?, x(2))?;
            x(0)
        }
        "spread-into" => {
            let target = x(0);
            let source = x(1);
            if !source.is_nullish() {
                for (k, v) in source.own_entries()? {
                    target.define_key(&k, v)?;
                }
            }
            target
        }
        "spread-object" => x(0).spread_object()?,
        "spread-arrays" => {
            let mut out = items(&x(0))?;
            out.extend(items(&x(1))?);
            Js::array(out)
        }
        "spread-append" => {
            let mut out = items(&x(0))?;
            out.push(x(1));
            Js::array(out)
        }
        "spread-array-into" => {
            for v in items(&x(1))? {
                x(0).push(v)?;
            }
            Js::Undefined
        }
        "array-push" => {
            x(0).push(x(1))?;
            Js::Undefined
        }
        "array-spread" => Js::array(items(&x(0))?),
        "destructure-two" => {
            let rows = items(&x(0))?;
            Js::array(vec![
                rows.first().cloned().unwrap_or(Js::Undefined),
                rows.get(1).cloned().unwrap_or(Js::Undefined),
            ])
        }
        "destructure-skip-first-take-second" => {
            items(&x(0))?.get(1).cloned().unwrap_or(Js::Undefined)
        }
        "destructure-data" => {
            if x(0).is_nullish() {
                return Err(Thrown::type_error(&format!(
                    "Cannot destructure property 'data' of '{}' as it is {}.",
                    x(0).to_display(),
                    x(0).to_display()
                )));
            }
            x(0).get("data")?
        }
        "template-string" | "to-property-key" => Js::String(key(0)?),
        "toString-method" => x(0).call_method("toString", vec![])?,
        "template-split-colon-index-one" | "split-colon-index-one" => text(&x(0))?
            .split(':')
            .nth(1)
            .map(Js::str)
            .unwrap_or(Js::Undefined),
        "split-comma" => Js::array(text(&x(0))?.split(',').map(Js::str).collect()),
        "split-double-underscore-first" => {
            Js::str(text(&x(0))?.split("__").next().unwrap_or_default())
        }
        "capture-replace" => x(0).get("replace")?,
        "concat-id" => js_add(&x(0), &x(1))?,
        "template-prefix" => template("", &[&x(0)], &["/"])?,
        "candle-topic" => template("candle_", &[&x(0), &x(1)], &["_"])?,
        "spot-candle-topic" => template("/market/candles:", &[&x(0), &x(1)], &["_"])?,
        "futures-candle-topic" => {
            template("/contractMarket/limitCandle:", &[&x(0), &x(1)], &["_"])?
        }
        "book-topic" => template("orderBook_", &[&x(0)], &[])?,
        "spot-book-topic" => template("/spotMarket/level2Depth50:", &[&x(0)], &[])?,
        "futures-book-topic" => template("/contractMarket/level2:", &[&x(0)], &[])?,
        "snapshot-error" => template("Failed to fetch orderbook snapshot for ", &[&x(0)], &[])?,
        "book-error" => template("Failed to fetch orderbook ", &[&x(0)], &[])?,
        "loaded-log" => template("Loaded ", &[&x(0), &x(1)], &[" KuCoin Futures "])?,
        "margin-mode-error" => template("Failed to fetch margin mode for ", &[&x(0)], &[])?,
        "market-not-found-message" | "template-missing-market" => {
            template("Market ", &[&x(0)], &[" not found"])?
        }
        "cancel-order-message" => template("Failed to cancel order ", &[&x(0)], &[])?,
        "cancel-symbol-message" => template("Failed to cancel orders for symbol ", &[&x(0)], &[])?,
        "parent-not-found-message" => template("Parent order not found for ", &[&x(0)], &[])?,
        "rejected-message" => template("KuCoin order rejected (code ", &[&x(0)], &[")"])?,
        "template-loaded-spot-markets" => template("Loaded ", &[&x(0)], &[" KuCoin Spot markets"])?,
        "template-unsupported-spot-order" => {
            template("KuCoin Spot does not support ", &[&x(0)], &[" orders"])?
        }
        "template-update-spot-order" => {
            template("Failed to update KuCoin Spot order ", &[&x(0)], &[])?
        }
        "Array.isArray" => Js::Bool(x(0).is_array()),
        "Object.keys-length" => {
            if x(0).is_nullish() {
                return Err(Thrown::type_error(
                    "Cannot convert undefined or null to object",
                ));
            }
            Js::Number(x(0).own_keys().len() as f64)
        }
        "Number.isFinite" => Js::Bool(matches!(x(0), Js::Number(v) if v.is_finite())),
        "typeof-string" => Js::Bool(x(0).is_string()),
        "typeof-object" => Js::Bool(x(0).type_of() == "object"),
        "strict-null" => Js::Bool(matches!(x(0), Js::Null)),
        "true" => Js::Bool(true),
        "false" => Js::Bool(false),
        "null" => Js::Null,
        "add" => js_add(&x(0), &x(1))?,
        "add-one" => js_add(&x(0), &Js::Number(1.0))?,
        "subtract" => Js::Number(n(0)? - n(1)?),
        "multiply" | "multiply-numbers" => Js::Number(n(0)? * n(1)?),
        "multiply-100" => Js::Number(n(0)? * 100.0),
        "divide" => Js::Number(n(0)? / n(1)?),
        "divide-1000" | "divide-thousand" => Js::Number(n(0)? / 1000.0),
        "remainder" => Js::Number(n(0)? % n(1)?),
        "mod-four-zero" => Js::Bool(n(0)? % 4.0 == 0.0),
        "lt" | "less-than" => Js::Bool(js_less(&x(0), &x(1))?),
        "lte" | "less-equal" => Js::Bool(js_less_equal(&x(0), &x(1))?),
        "greater-than" => Js::Bool(js_less(&x(1), &x(0))?),
        "gte" => Js::Bool(js_less_equal(&x(1), &x(0))?),
        "greater-zero" => Js::Bool(js_less(&Js::Number(0.0), &x(0))?),
        "greater-than-one" => Js::Bool(js_less(&Js::Number(1.0), &x(0))?),
        "greater-last-plus-one" => Js::Bool(js_less(&js_add(&x(1), &Js::Number(1.0))?, &x(0))?),
        other => {
            return Err(Thrown::error(
                "Error",
                &format!("KuCoin binding has no intrinsic {other}"),
            ));
        }
    })
}

/// Array.prototype.map over a live array; holes stay holes.
fn map(input: Js, f: Rc<dyn Fn(Js) -> Result<Js, Thrown>>) -> Result<Js, Thrown> {
    let Some(rows) = input.array_items() else {
        return Err(Thrown::type_error(&format!(
            "{}.map is not a function",
            input.to_display()
        )));
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(match row {
            Some(v) => Some(f(v)?),
            None => None,
        });
    }
    Ok(Js::holey_array(out))
}

fn filter(input: Js, f: Rc<dyn Fn(Js) -> Result<bool, Thrown>>) -> Result<Js, Thrown> {
    let Some(rows) = input.array_items() else {
        return Err(Thrown::type_error(&format!(
            "{}.filter is not a function",
            input.to_display()
        )));
    };
    let mut out = vec![];
    for v in rows.into_iter().flatten() {
        if f(v.clone())? {
            out.push(v);
        }
    }
    Ok(Js::array(out))
}

pub(crate) fn for_of(input: Js, f: &dyn Fn(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
    let iterator = input.iterator()?;
    let next = iterator.get("next")?;
    loop {
        let step = next.call(iterator.clone(), vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            return Ok(());
        }
        if let Err(error) = f(step.get("value")?) {
            // IteratorClose observes return; the original body throw takes precedence.
            if let Ok(close) = iterator.get("return")
                && !close.is_nullish()
            {
                let _ = close.call(iterator.clone(), vec![]);
            }
            return Err(error);
        }
    }
}

pub(crate) fn spawn_detached(
    weak: &W,
    body: safe_cex_exchange::js_runtime::promise::JsFuture<'static>,
) {
    if let Ok(scope) = scope_of(weak) {
        let reporter = scope.clone();
        let _ = Promise::spawn(
            &scope.exchange_runtime(),
            Box::pin(async move {
                if let Err(error) = body.await {
                    reporter.report(&error, "KuCoin unawaited task");
                }
                Ok(Js::Undefined)
            }),
        );
    }
}

/// Fresh language view over `receiver`: captures resolve `this` to it.
pub(crate) fn frame(weak: &W, receiver: &Js) -> Rc<StreamOps<Js, Thrown>> {
    let captures = weak.clone();
    let receiver = receiver.clone();
    let spawns = weak.clone();
    Rc::new(StreamOps {
        get: Rc::new(|v, k| v.get(k)),
        set: Rc::new(|v, k, value| v.set(k, value)),
        capture: Rc::new(move |path| capture(&captures, &receiver, path)),
        invoke: Rc::new(|f, args| f.call(Js::Undefined, args)),
        intrinsic: Rc::new(intrinsic),
        truthy: Rc::new(Js::truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(Js::strict_equals),
        literal: Rc::new(Js::str),
        number: Rc::new(Js::Number),
        undefined: Rc::new(|| Js::Undefined),
        array: Rc::new(Js::array),
        object: Rc::new(Js::object),
        callback: Rc::new(|name, _, body| {
            Js::function(name, move |_, a| {
                body(arg(&a, 0))?;
                Ok(Js::Undefined)
            })
        }),
        map: Rc::new(map),
        filter: Rc::new(filter),
        for_of: Rc::new(|input, f| for_of(input, &*f)),
        await_value: Rc::new(|v| Box::pin(await_js(v))),
        spawn: Rc::new(move |task| {
            spawn_detached(
                &spawns,
                Box::pin(async move {
                    task.await?;
                    Ok(Js::Undefined)
                }),
            )
        }),
        error_value: Rc::new(|e: &Thrown| e.value.clone()),
    })
}
