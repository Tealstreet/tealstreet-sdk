//! Concrete host traits and target adapters for this group of ported bodies:
//! REST actions, order placement/preparation and runtime orchestration.
use crate::binding::{
    globals::{arg, shared},
    promise::{JsFuture, Promise, await_js},
    rt::{Rt, Shared},
    value::{Js, Str, Thrown, from_typed, to_typed, utf16},
};
use crate::order_placement::OrderPlacementHost;
use crate::order_preparation::OrderPreparationHost;
use crate::rest_actions::{RestActionFuture, RestActionsHost};
use crate::runtime_orchestration::RuntimeOrchestrationHost;
use crate::{
    inverse_logic, order_logic, order_placement, order_preparation, rest_actions,
    runtime_orchestration,
};
use safe_cex_types::{
    Order, OrderSide, OrderStopTriggerType, OrderType, PlaceOrderOpts, Position, PositionMode,
    PositionSide,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

// ---------- JS operator helpers ----------

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// `${a}${b}...` template interpolation (ToString on each part).
fn template(parts: &[&Js]) -> Result<Js, Thrown> {
    let mut out: Vec<u16> = vec![];
    for part in parts {
        out.extend_from_slice(&part.to_js_string()?);
    }
    Ok(Js::String(out.into()))
}

fn lit(s: &str) -> Js {
    Js::str(s)
}

/// ToPrimitive (default/number hint): objects use their string form.
fn to_primitive(v: &Js) -> Result<Js, Thrown> {
    Ok(match v {
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::String(_) => v.clone(),
        other => Js::String(other.to_js_string()?),
    })
}

/// Abstract relational comparison `a < b`; None is the undefined (NaN) outcome.
fn less_than(a: &Js, b: &Js) -> Result<Option<bool>, Thrown> {
    let (pa, pb) = (to_primitive(a)?, to_primitive(b)?);
    if let (Js::String(x), Js::String(y)) = (&pa, &pb) {
        return Ok(Some(x[..] < y[..]));
    }
    let (x, y) = (pa.to_number()?, pb.to_number()?);
    if x.is_nan() || y.is_nan() {
        return Ok(None);
    }
    Ok(Some(x < y))
}

fn js_add(a: &Js, b: &Js) -> Result<Js, Thrown> {
    let (pa, pb) = (to_primitive(a)?, to_primitive(b)?);
    if pa.is_string() || pb.is_string() {
        return template(&[&pa, &pb]);
    }
    Ok(Js::Number(pa.to_number()? + pb.to_number()?))
}

fn num_op(args: &[Js], f: fn(f64, f64) -> f64) -> Result<Js, Thrown> {
    Ok(Js::Number(f(
        arg(args, 0).to_number()?,
        arg(args, 1).to_number()?,
    )))
}

fn property_key(key: &Js) -> Result<Str, Thrown> {
    key.to_js_string()
}

/// Optional-chaining read: nullish receivers yield undefined.
fn opt_get(v: &Js, k: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(k)
    }
}

/// `key in value`, walking the prototype chain of ordinary objects.
fn has_property(v: &Js, key: &str) -> Result<bool, Thrown> {
    if !v.is_object() {
        return Err(Thrown::type_error(&format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            v.to_display()
        )));
    }
    if v.has_own(key) {
        return Ok(true);
    }
    Ok(match v {
        Js::Object(o) => match o.proto.borrow().clone() {
            Some(proto) => has_property(&proto, key)?,
            None => false,
        },
        Js::Array(_) => {
            key == "length" || crate::binding::builtins::member(v, &utf16(key)).is_some()
        }
        _ => crate::binding::builtins::member(v, &utf16(key)).is_some(),
    })
}

/// `const { order, update } = value` property read with the V8 destructuring error.
fn destructure(v: &Js, key: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        return Err(Thrown::type_error(&format!(
            "Cannot destructure property '{key}' of '{}' as it is {}.",
            v.to_display(),
            v.to_display()
        )));
    }
    v.get(key)
}

fn not_iterable(v: &Js) -> Thrown {
    let what = match v {
        Js::Undefined | Js::Null => v.to_display(),
        Js::Number(_) | Js::Bool(_) => format!("{} {}", v.type_of(), v.to_display()),
        Js::Function(_) => "function".to_owned(),
        _ => "object".to_owned(),
    };
    Thrown::type_error(&format!(
        "{what} is not iterable (cannot read property Symbol(Symbol.iterator))"
    ))
}

/// Live native iterator over the iterable intrinsics.
pub(crate) struct JsIter(safe_cex_exchange::js_runtime::iterator::IteratorRecord);
fn iterate(value:&Js)->Result<JsIter,Thrown>{Ok(JsIter(safe_cex_exchange::js_runtime::iterator::IteratorRecord::get(value)?))}
fn iter_next(it:&mut JsIter)->Result<Option<Js>,Thrown>{it.0.step_value()}

fn array_len(v: &Js) -> usize {
    match v {
        Js::Array(items) => items.len(),
        _ => 0,
    }
}

/// HasProperty(i) then Get(i) on the live array.
fn element(v:&Js,i:usize)->Result<Option<Js>,Thrown>{if v.has_index(i){Ok(Some(v.get(&i.to_string())?))}else{Ok(None)}}
fn element_or_undefined(v:&Js,i:usize)->Result<Js,Thrown>{Ok(element(v,i)?.unwrap_or(Js::Undefined))}

/// Array callbacks run natively; a non-array receiver's member is not a builtin.
fn require_array(member: &Js, array: &Js, name: &str) -> Result<(), Thrown> {
    if array.is_array() {
        return Ok(());
    }
    if !member.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{}.{name} is not a function",
            array.type_of()
        )));
    }
    Err(Thrown::type_error(&format!(
        "Bitget binding: {name} over a non-array {} receiver has no native callback form",
        array.type_of()
    )))
}

/// `owner.member` captured with its owner as receiver; non-callables stay raw.
fn bind_callable(owner: Js, member: Js, name: &str) -> Js {
    if !member.is_callable() {
        return member;
    }
    Js::function(name, move |_, a| member.call(owner.clone(), a))
}

fn invoke_value(f: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
    if !f.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            f.to_display()
        )));
    }
    f.call(Js::Undefined, args)
}

/// Promise.prototype.finally: runs the callback, then re-settles with the original outcome.
fn promise_finally(rt: &Rt, promise: Promise) -> Js {
    let rt = rt.clone();
    Js::function("finally", move |_, a| {
        let callback = arg(&a, 0);
        let promise = promise.clone();
        let runtime = rt.exchange_runtime();
        Ok(rt.spawn(Box::pin(async move {
            let result = promise.wait().await;
            if callback.is_callable() {
                await_js(&runtime, callback.call(Js::Undefined, vec![])?).await?;
            }
            result
        })))
    })
}

/// lodash toInteger.
fn lodash_to_integer(v: &Js) -> Result<f64, Thrown> {
    if !v.truthy() {
        return Ok(0.0);
    }
    let mut n = v.to_number()?;
    if n.is_infinite() {
        n = n.signum() * 1.797_693_134_862_315_7e308;
    }
    if n.is_nan() {
        return Ok(0.0);
    }
    Ok(n.trunc())
}

/// Own then inherited enumerable keys (lodash getAllKeysIn for plain objects).
fn all_keys_in(v: &Js) -> Vec<Str> {
    let mut keys = v.own_keys();
    let mut proto = match v {
        Js::Object(o) => o.proto.borrow().clone(),
        _ => None,
    };
    while let Some(p) = proto {
        for k in p.own_keys() {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        proto = match &p {
            Js::Object(o) => o.proto.borrow().clone(),
            _ => None,
        };
    }
    keys
}

fn days_to_civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `new Date(value).toISOString()`.
fn date_iso(value: &Js) -> Result<Js, Thrown> {
    let ms = match value {
        Js::String(s) => safe_cex_num::js_date_parse_utf16(s, |local| local),
        other => other.to_number()?,
    };
    let ms = safe_cex_num::js_time_clip(ms);
    if ms.is_nan() {
        return Err(Thrown::error("RangeError", "Invalid time value"));
    }
    let days = (ms / 86_400_000.0).floor();
    let rem = (ms - days * 86_400_000.0) as i64;
    let (y, m, d) = days_to_civil(days as i64);
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else {
        format!("{}{:06}", if y < 0 { '-' } else { '+' }, y.abs())
    };
    Ok(Js::str(&format!(
        "{year}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )))
}

/// The `operation(name, args)` contract shared by every body in this group.
fn operation(name: &str, args: &[Js]) -> Result<Js, Thrown> {
    let a0 = arg(args, 0);
    let a1 = arg(args, 1);
    Ok(match name {
        "template" => template(&[&a0])?,
        "missing" => template(&[&a0, &lit(" "), &a1, &lit(" not found")])?,
        "suffix" => template(&[&a0, &a1])?,
        "greater" => Js::Bool(less_than(&a1, &a0)? == Some(true)),
        "less" => Js::Bool(less_than(&a0, &a1)? == Some(true)),
        "lessEqual" => Js::Bool(less_than(&a1, &a0)? == Some(false)),
        "add" => js_add(&a0, &a1)?,
        "subtract" => num_op(args, |a, b| a - b)?,
        "multiply" => num_op(args, |a, b| a * b)?,
        "divide" => num_op(args, |a, b| a / b)?,
        "remainder" => num_op(args, |a, b| a % b)?,
        "unaryPlus" => Js::Number(a0.to_number()?),
        "increment" => Js::Number(a0.to_number()? + 1.0),
        "destructureFirst" => iter_next(&mut iterate(&a0)?)?.unwrap_or(Js::Undefined),
        "objectSpread" => a0.spread_object()?,
        "typeofObject" => Js::Bool(a0.type_of() == "object"),
        "arrayAppendOwn" => {
            a0.push(a1)?;
            Js::Undefined
        }
        "restOmitSymbol" => {
            if a0.is_nullish() {
                return Err(Thrown::type_error(&format!(
                    "Cannot destructure '{}' as it is {}.",
                    a0.to_display(),
                    a0.to_display()
                )));
            }
            a0.get("symbol")?;
            let rest = Js::object(vec![]);
            for (k, v) in a0.own_entries()? {
                if k[..] != utf16("symbol")[..] {
                    rest.define_key(&k, v)?;
                }
            }
            rest
        }
        "absentMessage" => template(&[
            &lit("Bitget UTA strategy order "),
            &a0,
            &lit(" was already absent during cancel"),
        ])?,
        "parentMissing" => template(&[&lit("Parent order not found for "), &a0])?,
        "strategyPriceRequired" => template(&[
            &lit("Bitget UTA "),
            &a0,
            &lit(" orders require a trigger price."),
        ])?,
        "restoredMessage" => template(&[
            &lit("Bitget UTA update failed: "),
            &a0,
            &lit(". Original stop was restored; please retry the update."),
        ])?,
        "rollbackMessage" => template(&[
            &lit("Bitget UTA update failed and rollback also failed: "),
            &a0,
            &lit(" / "),
            &a1,
            &lit(". Your stop is not on the book — please re-add it."),
        ])?,
        "positionsFallbackMessage" => template(&[
            &lit("Could not load positions to size the "),
            &a0,
            &lit(" stop — placing it as a full-position stop: "),
            &a1,
        ])?,
        "environmentMessage" => template(&[
            &lit("API key environment mismatch: Your API key appears to be for "),
            &a0,
            &lit(" trading, but Tealstreet is configured for "),
            &a1,
            &lit(". Please check your Testnet setting."),
        ])?,
        "marketsLoadedMessage" => template(&[&lit("Loaded "), &a0, &lit(" Bitget markets")])?,
        "probeFallbackMessage" => template(&[
            &lit("Bitget account type probe failed; assuming Classic: "),
            &a0,
        ])?,
        "newDateISO" => date_iso(&a0)?,
        "incomeWarning" => template(&[
            &lit("Income history for "),
            &a0,
            &lit(".."),
            &a1,
            &lit(" is INCOMPLETE — "),
            &arg(args, 2),
            &lit(
                " chunk(s) could not be walked to completion. Any total derived from it is a lower bound.",
            ),
        ])?,
        "bookTopic" => template(&[&lit("orderBook_"), &a0])?,
        "candleTopic" => template(&[&lit("candle_"), &a0, &lit("_"), &a1])?,
        "candleChannel" => template(&[&lit("candle"), &a0])?,
        other => {
            return Err(Thrown::error(
                "Error",
                &format!("Bitget binding has no operation named {other}"),
            ));
        }
    })
}

// ---------- host trait implementations ----------

impl RestActionsHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn receiver(&self, path: &str) -> Result<Js, Thrown> {
        self.path(path)
    }
    fn set_receiver(&self, path: &str, value: Js) -> Result<(), Thrown> {
        self.set_path(path, value)
    }
    fn capture(&self, path: &str) -> Result<Js, Thrown> {
        let (owner, member) = self.owner_and_member(path)?;
        let name = path.rsplit('.').next().unwrap_or(path);
        Ok(bind_callable(owner, member, name))
    }
    fn capture_member(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        let member = value.get(name)?;
        if let (Js::Promise(p), true) = (value, member.is_undefined() && name == "finally") {
            return Ok(promise_finally(self, p.clone()));
        }
        Ok(bind_callable(value.clone(), member, name))
    }
    fn invoke(&self, method: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        invoke_value(&method, args)
    }
    fn start(&self, method: Js, args: Vec<Js>) -> Result<RestActionFuture<'_, Js, Thrown>, Thrown> {
        let value = invoke_value(&method, args)?;
        Ok(Box::pin(await_js(&self.exchange_runtime(), value)))
    }
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn key_get(&self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        value.get_key(&property_key(key)?)
    }
    fn has(&self, value: &Js, key: &str) -> Result<bool, Thrown> {
        has_property(value, key)
    }
    fn assign(&self, value: &Js, key: &str, field: Js) -> Result<(), Thrown> {
        value.set(key, field)
    }
    fn define(&self, value: &Js, key: &str, field: Js) -> Result<(), Thrown> {
        value.define(key, field)
    }
    fn delete(&self, value: &Js, key: &Js) -> Result<(), Thrown> {
        if value.is_nullish() {
            return Err(Thrown::type_error(
                "Cannot convert undefined or null to object",
            ));
        }
        value.delete(&property_key(key)?)?;
        Ok(())
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn filter<F>(&self, member: Js, array: Js, mut predicate: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js) -> Result<bool, Thrown>,
    {
        require_array(&member, &array, "filter")?;
        let out = Js::array(vec![]);
        for i in 0..array_len(&array) {
            if let Some(v) = element(&array, i)?
                && predicate(v.clone())?
            {
                out.push(v)?;
            }
        }
        Ok(out)
    }
    fn find<F>(&self, member: Js, array: Js, mut predicate: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js) -> Result<bool, Thrown>,
    {
        require_array(&member, &array, "find")?;
        for i in 0..array_len(&array) {
            let v = element_or_undefined(&array, i)?;
            if predicate(v.clone())? {
                return Ok(v);
            }
        }
        Ok(Js::Undefined)
    }
    fn some<F>(&self, member: Js, array: Js, mut predicate: F) -> Result<bool, Thrown>
    where
        F: FnMut(Js) -> Result<bool, Thrown>,
    {
        require_array(&member, &array, "some")?;
        for i in 0..array_len(&array) {
            if let Some(v) = element(&array, i)?
                && predicate(v)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn operation(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        operation(name, &args)
    }
    fn new_error(&self, constructor: Js, message: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::new(invoke_value(&constructor, vec![message])?))
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
}

impl OrderPlacementHost for Rt {
    type Iterator = JsIter;
    fn spread_into(&self, array: &Js, elements: Js) -> Result<(), Thrown> {
        let mut it = iterate(&elements)?;
        while let Some(v) = iter_next(&mut it)? {
            array.push(v)?;
        }
        Ok(())
    }
    fn iterator(&self, value: &Js) -> Result<JsIter, Thrown> {
        iterate(value)
    }
    fn next(&self, iterator: &mut JsIter) -> Result<Option<Js>, Thrown> {
        iter_next(iterator)
    }
    fn pair(&self, value: Js) -> Result<(Js, Js), Thrown> {
        let mut it = iterate(&value)?;
        let first = iter_next(&mut it)?.unwrap_or(Js::Undefined);
        let second = iter_next(&mut it)?.unwrap_or(Js::Undefined);
        Ok((first, second))
    }
    fn map<F>(&self, member: Js, array: Js, mut callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js) -> Result<Js, Thrown>,
    {
        require_array(&member, &array, "map")?;
        let len = array_len(&array);
        let mut out: Vec<Option<Js>> = vec![None; len];
        for (i, slot) in out.iter_mut().enumerate() {
            if let Some(v) = element(&array, i)? {
                *slot = Some(callback(v)?);
            }
        }
        Ok(Js::holey_array(out))
    }
    fn reduce<F>(&self, member: Js, array: Js, initial: Js, mut callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, Js) -> Result<Js, Thrown>,
    {
        require_array(&member, &array, "reduce")?;
        let mut acc = initial;
        for i in 0..array_len(&array) {
            if let Some(v) = element(&array, i)? {
                acc = callback(acc, v)?;
            }
        }
        Ok(acc)
    }
    fn for_each<F>(&self, member: Js, array: Js, mut callback: F) -> Result<(), Thrown>
    where
        F: FnMut(Js) -> Result<(), Thrown>,
    {
        require_array(&member, &array, "forEach")?;
        for i in 0..array_len(&array) {
            if let Some(v) = element(&array, i)? {
                callback(v)?;
            }
        }
        Ok(())
    }
    fn assign_key(&self, value: &Js, key: &Js, field: Js) -> Result<(), Thrown> {
        let key = property_key(key)?;
        value.set_key(&key, field)
    }
}

impl OrderPreparationHost for Rt {
    fn times<F>(&self, _method: Js, count: Js, mut callback: F) -> Result<Js, Thrown>
    where
        F: FnMut() -> Result<Js, Thrown>,
    {
        const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
        const MAX_ARRAY_LENGTH: f64 = 4_294_967_295.0;
        let n = lodash_to_integer(&count)?;
        if !(1.0..=MAX_SAFE_INTEGER).contains(&n) {
            return Ok(Js::array(vec![]));
        }
        let length = n.min(MAX_ARRAY_LENGTH) as u64;
        let mut out = Vec::with_capacity(length.min(1 << 16) as usize);
        for _ in 0..length {
            out.push(callback()?);
        }
        let mut extra = n - MAX_ARRAY_LENGTH;
        while extra > 0.0 {
            callback()?;
            extra -= 1.0;
        }
        Ok(Js::array(out))
    }
    fn launch(&self, body: RestActionFuture<'static, Js, Thrown>) -> Result<Js, Thrown> {
        Ok(self.spawn(body))
    }
    fn await_value(&self, promise: Js) -> RestActionFuture<'_, Js, Thrown> {
        Box::pin(await_js(&self.exchange_runtime(), promise))
    }
}

impl RuntimeOrchestrationHost for Rt {
    fn first(&self, value: Js) -> Result<Js, Thrown> {
        Ok(iter_next(&mut iterate(&value)?)?.unwrap_or(Js::Undefined))
    }
    fn function<F>(&self, body: F) -> Js
    where
        F: Fn(Vec<Js>) -> Result<Js, Thrown> + 'static,
    {
        // Bodies index their declared parameters; a missing argument reads as undefined.
        Js::function("", move |_, mut a| {
            if a.is_empty() {
                a.push(Js::Undefined);
            }
            body(a)
        })
    }
}

// ---------- module imports these bodies read ----------

fn is_bitget_success_code(code: &Js) -> Result<bool, Thrown> {
    let code = if code.is_nullish() {
        Js::Number(0.0)
    } else {
        code.clone()
    };
    Ok(safe_cex_num::parse_int(&text(&code)?, Some(10)) == 0.0)
}

fn to_finite_number(value: &Js, fallback: Js) -> Result<Js, Thrown> {
    let fallback = if fallback.is_undefined() {
        Js::Number(0.0)
    } else {
        fallback
    };
    Ok(match value {
        Js::Number(n) if n.is_finite() => value.clone(),
        Js::String(s) => {
            let s = String::from_utf16_lossy(s);
            if safe_cex_num::js_trim(&s).is_empty() {
                return Ok(fallback);
            }
            let parsed = safe_cex_num::parse_float(&s);
            if parsed.is_finite() {
                Js::Number(parsed)
            } else {
                fallback
            }
        }
        _ => fallback,
    })
}

fn lodash_values(collection: &Js) -> Result<Vec<Js>, Thrown> {
    Ok(match collection {
        Js::Array(_) => collection.array_values()?.unwrap_or_default(),
        Js::Undefined | Js::Null => vec![],
        Js::String(_) => collection
            .own_entries()?
            .into_iter()
            .map(|(_, v)| v)
            .collect(),
        other if other.is_object() => other.own_entries()?.into_iter().map(|(_, v)| v).collect(),
        _ => vec![],
    })
}

fn lodash_property(value: &Js, key: &Js) -> Result<Js, Thrown> {
    if value.is_nullish() {
        return Ok(Js::Undefined);
    }
    value.get_key(&property_key(key)?)
}

/// lodash iteratee shorthand: functions are called, anything else is a property path.
fn lodash_iteratee(iteratee: &Js, value: &Js) -> Result<Js, Thrown> {
    if iteratee.is_callable() {
        iteratee.call(Js::Undefined, vec![value.clone()])
    } else {
        lodash_property(value, iteratee)
    }
}

fn update_order_book(book: &Js, side: &Js, update: &Js, upsert: &Js) -> Result<Js, Thrown> {
    let upsert = upsert.is_undefined() || upsert.truthy();
    let levels = book.get_key(&property_key(side)?)?;
    let descending = side.strict_equals(&Js::str("bids"));
    let price = update.get("price")?;
    let (mut left, mut right) = (0_i64, array_len(&levels) as i64 - 1);
    let mut found = None;
    while left <= right {
        let mid = ((left + right) as u64 >> 1) as i64;
        let mid_price = element_or_undefined(&levels, mid as usize)?.get("price")?;
        if mid_price.strict_equals(&price) {
            found = Some(mid as usize);
            break;
        }
        let toward_right = if descending {
            less_than(&price, &mid_price)? == Some(true)
        } else {
            less_than(&mid_price, &price)? == Some(true)
        };
        if toward_right {
            left = mid + 1
        } else {
            right = mid - 1
        }
    }
    let zero = update.get("amount")?.strict_equals(&Js::Number(0.0));
    let Js::Array(items) = &levels else {
        return Err(Thrown::type_error("levels.splice is not a function"));
    };
    Ok(Js::Number(match found {
        None if !upsert || zero => -1.0,
        None => {
            let at = (left as usize).min(items.len());
            levels.splice_array(at,0,vec![update.clone()])?;
            at as f64
        }
        Some(index) if zero => {
            levels.splice_array(index,1,vec![])?;
            -1.0
        }
        Some(index) => {
            element_or_undefined(&levels, index)?.set("amount", update.get("amount")?)?;
            index as f64
        }
    }))
}

fn calc_order_book_total(book: &Js) -> Result<(), Thrown> {
    for side in ["bids", "asks"] {
        let levels = book.get(side)?;
        let mut total = Js::Number(0.0);
        for i in 0..array_len(&levels) {
            let level = element_or_undefined(&levels, i)?;
            total = js_add(&total, &level.get("amount")?)?;
            level.set("total", total.clone())?;
        }
    }
    Ok(())
}

fn exchange_error(e: safe_cex_exchange::runtime::ExchangeError) -> Thrown {
    Thrown::from(e)
}

fn place_opts_typed(opts: &Js) -> Result<PlaceOrderOpts, Thrown> {
    to_typed(opts)
}

fn paginate_income_range(rt: Rt, fetch: Js, opts: Js) -> JsFuture<'static> {
    Box::pin(async move {
        use safe_cex_exchange::income_paging::{IncomePageParams, IncomeRangeOptions};
        let field = |k: &str| -> Result<f64, Thrown> { opts.get(k)?.to_number() };
        let optional = |k: &str| -> Result<Option<f64>, Thrown> {
            let v = opts.get(k)?;
            if v.is_nullish() {
                Ok(None)
            } else {
                Ok(Some(v.to_number()?))
            }
        };
        let options = IncomeRangeOptions {
            start_time: field("startTime")?,
            end_time: field("endTime")?,
            chunk_ms: field("chunkMs")?,
            page_limit: field("pageLimit")?,
            chunk_delay_ms: optional("chunkDelayMs")?,
            max_pages_per_chunk: optional("maxPagesPerChunk")?,
        };
        let page_runtime = rt.exchange_runtime();
        let mut page = move |p: IncomePageParams| -> safe_cex_exchange::runtime::ExchangeFuture<
            'static,
            Vec<safe_cex_types::IncomeRecord>,
        > {
            let fetch = fetch.clone();
            Box::pin({
                let await_runtime = page_runtime.clone();
                async move {
                    let request = Js::object(vec![
                        ("startTime", Js::Number(p.start_time)),
                        ("endTime", Js::Number(p.end_time)),
                        ("limit", Js::Number(p.limit)),
                    ]);
                    let rows =
                        await_js(&await_runtime, invoke_value(&fetch, vec![request])?).await?;
                    Ok(to_typed(&rows)?)
                }
            })
        };
        let runtime = rt.exchange_runtime();
        let mut delay = move |ms: f64| {
            runtime.sleep(if ms.is_finite() && ms > 0.0 {
                ms as u64
            } else {
                0
            })
        };
        let result =
            safe_cex_exchange::income_paging::paginate_income_range(&mut page, options, &mut delay)
                .await
                .map_err(exchange_error)?;
        Ok(Js::object(vec![
            ("records", from_typed(&result.records)?),
            ("truncated", Js::Bool(result.truncated)),
            ("exhaustedChunks", Js::Number(result.exhausted_chunks)),
            ("requests", Js::Number(result.requests)),
        ]))
    })
}

pub(crate) fn register(globals: &Js, weak: Weak<Shared>) {
    let def = |k: &str, v: Js| {
        let _ = globals.define(k, v);
    };
    def(
        "uuid",
        Js::function("uuid", |_, _| {
            safe_cex_crypto::uuid::uuid()
                .map(|id| Js::str(&id))
                .map_err(|e| Thrown::error("Error", &e))
        }),
    );
    def(
        "adjust",
        Js::function("adjust", |_, a| {
            let value = arg(&a, 0).to_number()?;
            let step = arg(&a, 1);
            let step: safe_cex_num::PrecisionInput = match &step {
                Js::String(s) => String::from_utf16_lossy(s).into(),
                Js::Undefined | Js::Null => safe_cex_num::PrecisionInput::String(text(&step)?),
                other => other.to_number()?.into(),
            };
            Ok(Js::Number(safe_cex_num::adjust(value, step)))
        }),
    );
    def(
        "inverseObj",
        Js::function("inverseObj", |_, a| {
            let source = arg(&a, 0);
            if source.is_nullish() {
                return Err(Thrown::type_error(
                    "Cannot convert undefined or null to object",
                ));
            }
            let mut acc = Js::object(vec![]);
            for (k, v) in source.own_entries()? {
                let next = acc.spread_object()?;
                next.define_key(&property_key(&v)?, Js::String(k))?;
                acc = next;
            }
            Ok(acc)
        }),
    );
    def(
        "omitUndefined",
        Js::function("omitUndefined", |_, a| {
            let source = arg(&a, 0);
            let out = Js::object(vec![]);
            if !source.is_nullish() {
                for k in all_keys_in(&source) {
                    let v = source.get_key(&k)?;
                    if !v.is_undefined() {
                        out.define_key(&k, v)?;
                    }
                }
            }
            Ok(out)
        }),
    );
    def(
        "omit",
        Js::function("omit", |_, a| {
            let source = arg(&a, 0);
            let out = Js::object(vec![]);
            if source.is_nullish() {
                return Ok(out);
            }
            for k in all_keys_in(&source) {
                out.define_key(&k, source.get_key(&k)?)?;
            }
            for path in a.iter().skip(1) {
                let paths = path.array_values()?.unwrap_or_else(|| vec![path.clone()]);
                for p in paths {
                    out.delete(&property_key(&p)?)?;
                }
            }
            Ok(out)
        }),
    );
    def(
        "partition",
        Js::function("partition", |_, a| {
            let (truthy, falsy) = (Js::array(vec![]), Js::array(vec![]));
            let predicate = arg(&a, 1);
            for v in lodash_values(&arg(&a, 0))? {
                let target = if lodash_iteratee(&predicate, &v)?.truthy() {
                    &truthy
                } else {
                    &falsy
                };
                target.push(v)?;
            }
            Ok(Js::array(vec![truthy, falsy]))
        }),
    );
    def(
        "groupBy",
        Js::function("groupBy", |_, a| {
            let out = Js::object(vec![]);
            let iteratee = arg(&a, 1);
            for v in lodash_values(&arg(&a, 0))? {
                let key = property_key(&lodash_iteratee(&iteratee, &v)?)?;
                let key_text = String::from_utf16_lossy(&key);
                if out.has_own(&key_text) {
                    out.get_key(&key)?.push(v)?;
                } else {
                    out.define_key(&key, Js::array(vec![v]))?;
                }
            }
            Ok(out)
        }),
    );
    def(
        "chunk",
        Js::function("chunk", |_, a| {
            let size = lodash_to_integer(&arg(&a, 1))?.max(0.0);
            let values = arg(&a, 0).array_values()?.unwrap_or_default();
            if values.is_empty() || size < 1.0 {
                return Ok(Js::array(vec![]));
            }
            Ok(Js::array(
                values
                    .chunks(size as usize)
                    .map(|c| Js::array(c.to_vec()))
                    .collect(),
            ))
        }),
    );
    def(
        "uniqBy",
        Js::function("uniqBy", |_, a| {
            let iteratee = arg(&a, 1);
            let (mut seen, mut out): (Vec<Js>, Vec<Js>) = (vec![], vec![]);
            for v in arg(&a, 0).array_values()?.unwrap_or_default() {
                let computed = lodash_iteratee(&iteratee, &v)?;
                if !seen.iter().any(|s| s.same_value_zero(&computed)) {
                    seen.push(computed);
                    out.push(v);
                }
            }
            Ok(Js::array(out))
        }),
    );
    def(
        "times",
        Js::function("times", |_, a| {
            let n = lodash_to_integer(&arg(&a, 0))?;
            if !(1.0..=9_007_199_254_740_991.0).contains(&n) {
                return Ok(Js::array(vec![]));
            }
            let iteratee = arg(&a, 1);
            let mut out = vec![];
            for i in 0..n.min(4_294_967_295.0) as u64 {
                let index = Js::Number(i as f64);
                out.push(if iteratee.is_callable() {
                    iteratee.call(Js::Undefined, vec![index])?
                } else {
                    index
                });
            }
            Ok(Js::array(out))
        }),
    );
    def(
        "isBitgetSuccessCode",
        Js::function("isBitgetSuccessCode", |_, a| {
            Ok(Js::Bool(is_bitget_success_code(&arg(&a, 0))?))
        }),
    );
    def(
        "getBitgetApiErrorMessage",
        Js::function("getBitgetApiErrorMessage", |_, a| {
            let msg = opt_get(&arg(&a, 0), "msg")?;
            Ok(if msg.truthy() { msg } else { arg(&a, 1) })
        }),
    );
    def(
        "isBitgetEnvironmentMismatch",
        Js::function("isBitgetEnvironmentMismatch", |_, a| {
            let message = arg(&a, 0);
            Ok(Js::Bool(match &message {
                Js::Undefined | Js::Null => false,
                Js::String(s) => String::from_utf16_lossy(s)
                    .to_lowercase()
                    .contains("environment is incorrect"),
                _ => return Err(Thrown::type_error("message?.toLowerCase is not a function")),
            }))
        }),
    );
    def(
        "isBitgetUtaStrategyOrderNotFound",
        Js::function("isBitgetUtaStrategyOrderNotFound", |_, a| {
            Ok(Js::Bool(match arg(&a, 0) {
                Js::String(s) => String::from_utf16_lossy(&s)
                    .to_lowercase()
                    .contains("corresponding strategy id could not be found"),
                _ => false,
            }))
        }),
    );
    def(
        "toFiniteNumber",
        Js::function("toFiniteNumber", |_, a| {
            to_finite_number(&arg(&a, 0), arg(&a, 1))
        }),
    );
    let w = weak.clone();
    def(
        "getTickInterval",
        Js::function("getTickInterval", move |_, _| {
            Ok(shared(&w)?
                .base()
                .get_tick_interval()
                .map_or(Js::Undefined, Js::Number))
        }),
    );
    let w = weak.clone();
    def(
        "resolveCallerClientOrderId",
        Js::function("resolveCallerClientOrderId", move |_, a| {
            let opts = arg(&a, 0);
            if opts.get("clientOrderId")?.is_undefined() {
                return Ok(Js::Undefined);
            }
            let typed = place_opts_typed(&opts)?;
            let id = shared(&w)?
                .base()
                .resolve_caller_client_order_id(&typed)
                .map_err(exchange_error)?;
            Ok(id.map_or(Js::Undefined, |id| Js::str(&id)))
        }),
    );
    let w = weak.clone();
    def(
        "assertCallerClientOrderIds",
        Js::function("assertCallerClientOrderIds", move |_, a| {
            let mut typed = vec![];
            let mut it = iterate(&arg(&a, 0))?;
            while let Some(order) = iter_next(&mut it)? {
                if !order.get("clientOrderId")?.is_undefined() {
                    typed.push(place_opts_typed(&order)?);
                }
            }
            shared(&w)?
                .base()
                .assert_caller_client_order_ids(&typed)
                .map_err(exchange_error)?;
            Ok(Js::Undefined)
        }),
    );
    let w = weak.clone();
    def(
        "loop",
        Js::function("loop", move |_, a| {
            let rt = shared(&w)?;
            let callback = arg(&a, 0);
            let min = arg(&a, 1);
            if min.strict_equals(&Js::Number(f64::INFINITY)) {
                return Ok(Js::Promise(Promise::settled(
                    &rt.exchange_runtime(),
                    Ok(Js::Undefined),
                )));
            }
            // The TS fallback also console.warns once per caller; there is no console here.
            let interval = if min.truthy() && less_than(&Js::Number(0.0), &min)? == Some(true) {
                min.to_number()?
            } else {
                5000.0
            };
            let timer = rt.clone();
            Ok(rt.spawn(Box::pin(async move {
                timer
                    .exchange_runtime()
                    .sleep(if interval.is_finite() {
                        interval as u64
                    } else {
                        u64::MAX
                    })
                    .await
                    .map_err(exchange_error)?;
                let callback = callback.clone();
                timer.set_timeout(
                    Rc::new(move || invoke_value(&callback, vec![]).map(|_| ())),
                    0.0,
                );
                Ok(Js::Undefined)
            })))
        }),
    );
    let w = weak.clone();
    def(
        "paginateIncomeRange",
        Js::function("paginateIncomeRange", move |_, a| {
            let rt = shared(&w)?;
            Ok(rt.spawn(paginate_income_range(rt.clone(), arg(&a, 0), arg(&a, 1))))
        }),
    );
    def(
        "updateOrderBook",
        Js::function("updateOrderBook", |_, a| {
            update_order_book(&arg(&a, 0), &arg(&a, 1), &arg(&a, 2), &arg(&a, 3))
        }),
    );
    def(
        "calcOrderBookTotal",
        Js::function("calcOrderBookTotal", |_, a| {
            calc_order_book_total(&arg(&a, 0))?;
            Ok(Js::Undefined)
        }),
    );
}

// ---------- typed boundaries of the pure order decisions ----------

fn side_of(v: &Js) -> OrderSide {
    if v.strict_equals(&Js::str("buy")) {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}

fn mode_of(v: &Js) -> PositionMode {
    if v.strict_equals(&Js::str("hedged")) {
        PositionMode::Hedged
    } else {
        PositionMode::OneWay
    }
}

fn enum_js<T: serde::Serialize>(value: &T) -> Result<Js, Thrown> {
    from_typed(value)
}

fn order_type_of(v: &Js) -> Option<OrderType> {
    if !v.is_string() {
        return None;
    }
    to_typed(v).ok()
}

/// Amount step of `store.marketsBySymbol[symbol]?.precision?.amount`, None when falsy.
fn amount_step(rt: &Rt, symbol: &Js) -> Result<Option<f64>, Thrown> {
    let market = rt
        .path("store.marketsBySymbol")?
        .get_key(&property_key(symbol)?)?;
    let step = opt_get(&opt_get(&market, "precision")?, "amount")?;
    Ok(if step.truthy() {
        Some(step.to_number()?)
    } else {
        None
    })
}

type AlgoSideBody = fn(
    OrderType,
    OrderSide,
    PositionMode,
) -> Result<&'static str, safe_cex_exchange::runtime::ExchangeError>;

fn algo_order_side(opts: &Js, mode: &Js, body: AlgoSideBody) -> Result<Js, Thrown> {
    let kind = opts.get("type")?;
    let side = opts.get("side")?;
    let typed = order_type_of(&kind).filter(|t| {
        matches!(
            t,
            OrderType::TakeProfit | OrderType::StopLoss | OrderType::TrailingStopLoss
        )
    });
    let known_side = side.strict_equals(&Js::str("buy")) || side.strict_equals(&Js::str("sell"));
    match typed {
        Some(t) if known_side => Ok(Js::str(
            body(t, side_of(&side), mode_of(mode)).map_err(exchange_error)?,
        )),
        _ => Err(Thrown::new(crate::binding::value::error_object(
            "Error",
            &text(&template(&[
                &lit("Unknown algo order side: "),
                &kind,
                &lit(" - "),
                &side,
            ])?)?,
        ))),
    }
}

/// The four PlaceOrderOpts/Order fields the plan selection reads.
fn selection_opts(opts: &Js, strict: bool) -> Result<(PlaceOrderOpts, Option<Js>), Thrown> {
    let kind = opts.get("type")?;
    let typed_kind = order_type_of(&kind);
    let amount = opts.get("amount")?;
    let amount = if strict {
        to_typed::<f64>(&amount)?
    } else {
        amount.as_f64().unwrap_or(f64::NAN)
    };
    let selection = PlaceOrderOpts {
        symbol: if strict {
            to_typed(&opts.get("symbol")?)?
        } else {
            text(&opts.get("symbol")?)?
        },
        r#type: typed_kind.unwrap_or(OrderType::Limit),
        side: side_of(&opts.get("side")?),
        price: None,
        amount,
        reduce_only: None,
        time_in_force: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        trigger_type: None,
        implied_volatility: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
    };
    Ok((
        selection,
        if typed_kind.is_none() {
            Some(kind)
        } else {
            None
        },
    ))
}

/// Runs a typed body with an emitAndLogError callback; the first emit failure wins.
fn with_emitter<T>(
    rt: &Rt,
    body: impl FnOnce(&mut dyn FnMut(String)) -> Result<T, safe_cex_exchange::runtime::ExchangeError>,
) -> Result<T, Thrown> {
    let failure: RefCell<Option<Thrown>> = RefCell::new(None);
    let mut emit = |message: String| {
        if failure.borrow().is_some() {
            return;
        }
        if let Err(e) = rt.call_path("emitAndLogError", vec![Js::str(&message)]) {
            *failure.borrow_mut() = Some(e);
        }
    };
    let result = body(&mut emit);
    if let Some(e) = failure.take() {
        return Err(e);
    }
    result.map_err(exchange_error)
}

fn select_plan_type_and_amount(rt: &Rt, opts: &Js, positions: &Js) -> Result<Js, Thrown> {
    let (selection, raw_kind) = selection_opts(opts, true)?;
    let typed_positions: Vec<Position> = to_typed(positions)?;
    let step = amount_step(rt, &opts.get("symbol")?)?;
    let result = with_emitter(rt, |emit| {
        order_logic::select_plan_type_and_amount(&selection, &typed_positions, step, emit)
    });
    match (result, raw_kind) {
        (Ok((plan, amount)), _) => Ok(Js::array(vec![Js::str(plan), Js::Number(amount)])),
        (Err(e), Some(kind)) if e.message().starts_with("Unknown order type: ") => {
            Err(Thrown::new(crate::binding::value::error_object(
                "Error",
                &text(&template(&[&lit("Unknown order type: "), &kind])?)?,
            )))
        }
        (Err(e), _) => Err(e),
    }
}

fn cancel_plan_type(rt: &Rt, order: &Js) -> Result<Js, Thrown> {
    let typed: Order = to_typed(order)?;
    let positions: Vec<Position> = to_typed(&rt.path("store.positions")?)?;
    let step = amount_step(rt, &order.get("symbol")?)?;
    let hit = with_emitter(rt, |emit| {
        order_logic::get_cancel_plan_type(&typed, &positions, step, emit)
    })?;
    Ok(match hit {
        safe_cex_num::TableHit::Mapped(plan) => Js::str(plan),
        safe_cex_num::TableHit::Inherited(safe_cex_num::ProtoMember::Prototype) => {
            Js::object(vec![])
        }
        safe_cex_num::TableHit::Inherited(member) => {
            let name = member
                .js_string()
                .trim_start_matches("function ")
                .split('(')
                .next()
                .unwrap_or_default()
                .to_owned();
            Js::function(&name, |_, _| Ok(Js::Undefined))
        }
        safe_cex_num::TableHit::Missing => Js::Undefined,
    })
}

async fn order_plan_type_and_amount(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    let (selection, _) = selection_opts(&opts, false)?;
    let chosen: RefCell<Js> = RefCell::new(Js::Undefined);
    let stashed: RefCell<Option<Thrown>> = RefCell::new(None);
    let store_positions = || -> Vec<Position> {
        let read = rt.path("store.positions").and_then(|v| {
            *chosen.borrow_mut() = v.clone();
            to_typed::<Vec<Position>>(&v)
        });
        read.unwrap_or_else(|e| {
            *stashed.borrow_mut() = Some(e);
            vec![]
        })
    };
    let resolve = |symbol: &str, side: PositionSide| {
        let symbol = symbol.to_owned();
        let chosen = &chosen;
        {
            let await_runtime = rt.exchange_runtime();
            async move {
                let value = rt.call_path(
                    "resolvePositionsForPlanType",
                    vec![Js::str(&symbol), enum_js(&side)?],
                )?;
                let value = await_js(&await_runtime, value).await?;
                *chosen.borrow_mut() = value.clone();
                to_typed::<Vec<Position>>(&value)
            }
        }
    };
    let select = |_: order_logic::PlanSelectionOptions<'_>, _: &[Position]| -> Result<Js, Thrown> {
        if let Some(e) = stashed.borrow_mut().take() {
            return Err(e);
        }
        let positions = chosen.borrow().clone();
        rt.call_path("selectPlanTypeAndAmount", vec![opts.clone(), positions])
    };
    order_logic::get_order_plan_type_and_amount(
        order_logic::PlanSelectionOptions::Place(&selection),
        store_positions,
        resolve,
        select,
    )
    .await
}

// ---------- manifest target adapters ----------

macro_rules! async_member {
    ($rt:expr, $name:expr, |$h:ident, $a:ident| $body:block) => {{
        let rt: Rt = $rt.clone();
        Js::function($name, move |_, $a: Vec<Js>| {
            let owned = rt.clone();
            Ok(rt.spawn(Box::pin(async move {
                let $h: &Rt = &owned;
                let $a = $a;
                $body
            })))
        })
    }};
}

macro_rules! sync_member {
    ($rt:expr, $name:expr, |$h:ident, $a:ident| $body:block) => {{
        let rt: Rt = $rt.clone();
        Js::function($name, move |_, $a: Vec<Js>| {
            let $h: &Rt = &rt;
            $body
        })
    }};
}

pub(crate) fn bind(
    rt: &Rt,
    owner: &str,
    name: &str,
    _targets: &[&str],
) -> Result<Option<Js>, Thrown> {
    let f = match (owner, name) {
        ("BitgetExchange", "validateAccount") => {
            async_member!(rt, name, |h, _a| {
                runtime_orchestration::validate_account(h).await
            })
        }
        ("BitgetExchange", "start") => {
            async_member!(rt, name, |h, _a| { runtime_orchestration::start(h).await })
        }
        ("BitgetExchange", "tick") => {
            async_member!(rt, name, |h, a| {
                runtime_orchestration::tick(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "_detectAccountType") => {
            async_member!(rt, name, |h, _a| {
                runtime_orchestration::detect_account_type(h).await
            })
        }
        ("BitgetExchange", "ensureAccountTypeDetected") => {
            async_member!(rt, name, |h, _a| {
                runtime_orchestration::ensure_account_type_detected(h).await
            })
        }
        ("BitgetExchange", "fetchIncomeHistoryRange") => async_member!(rt, name, |h, a| {
            runtime_orchestration::fetch_income_history_range(h, arg(&a, 0), arg(&a, 1), arg(&a, 2))
                .await
        }),
        ("BitgetExchange", "cancelUtaStrategyOrders") => async_member!(rt, name, |h, a| {
            order_placement::cancel_uta_strategy_orders(h, arg(&a, 0), arg(&a, 1)).await
        }),
        ("BitgetExchange", "placeOrder") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "placeOrders") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_orders(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "formatCreateOrder") => {
            async_member!(rt, name, |h, a| {
                order_preparation::format_create_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "formatUtaCreateOrder") => {
            async_member!(rt, name, |h, a| {
                order_preparation::format_uta_create_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "formatUtaStrategyOrder") => {
            async_member!(rt, name, |h, a| {
                order_preparation::format_uta_strategy_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "formatAlgoOrder") => {
            async_member!(rt, name, |h, a| {
                order_preparation::format_algo_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetInverseExchange", "formatAlgoOrder") => {
            async_member!(rt, name, |h, a| {
                order_preparation::inverse_format_algo_order(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "placeOrderBatch") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_order_batch(h, arg(&a, 0)).await
            })
        }
        ("BitgetInverseExchange", "placeOrderBatch") => {
            async_member!(rt, name, |h, a| {
                order_placement::inverse_place_order_batch(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "placeUtaOrders") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_uta_orders(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "placeUtaStrategyOrders") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_uta_strategy_orders(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "placeAlgoOrders") => {
            async_member!(rt, name, |h, a| {
                order_placement::place_algo_orders(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "updateVirtualTpSlOrder") => async_member!(rt, name, |h, a| {
            let input = arg(&a, 0);
            destructure(&input, "order")?;
            order_preparation::update_virtual_tp_sl_order(h, input, arg(&a, 1)).await
        }),
        ("BitgetExchange", "tryAmendLimitOrder") => async_member!(rt, name, |h, a| {
            rest_actions::try_amend_limit_order(h, arg(&a, 0), arg(&a, 1)).await
        }),
        ("BitgetExchange", "updateOrder") => async_member!(rt, name, |h, a| {
            let input = arg(&a, 0);
            let (order, update) = (
                destructure(&input, "order")?,
                destructure(&input, "update")?,
            );
            rest_actions::update_order(h, order, update).await
        }),
        ("BitgetExchange", "updateUtaStrategyOrder") => async_member!(rt, name, |h, a| {
            let input = arg(&a, 0);
            destructure(&input, "order")?;
            order_preparation::update_uta_strategy_order(h, input).await
        }),
        ("BitgetExchange", "updateAlgoOrder") => async_member!(rt, name, |h, a| {
            let input = arg(&a, 0);
            let (order, update) = (
                destructure(&input, "order")?,
                destructure(&input, "update")?,
            );
            rest_actions::update_algo_order(h, order, update).await
        }),
        ("BitgetInverseExchange", "updateAlgoOrder") => async_member!(rt, name, |h, a| {
            let input = arg(&a, 0);
            let (order, update) = (
                destructure(&input, "order")?,
                destructure(&input, "update")?,
            );
            rest_actions::inverse_update_algo_order(h, order, update).await
        }),
        ("BitgetExchange", "setLeverage") => async_member!(rt, name, |h, a| {
            rest_actions::set_leverage(h, arg(&a, 0), arg(&a, 1), arg(&a, 2)).await
        }),
        ("BitgetInverseExchange", "setLeverage") => async_member!(rt, name, |h, a| {
            rest_actions::inverse_set_leverage(h, arg(&a, 0), arg(&a, 1), arg(&a, 2)).await
        }),
        ("BitgetExchange", "changeMarginType") => async_member!(rt, name, |h, a| {
            rest_actions::change_margin_type(h, arg(&a, 0), arg(&a, 1)).await
        }),
        ("BitgetInverseExchange", "changeMarginType") => async_member!(rt, name, |h, a| {
            rest_actions::inverse_change_margin_type(h, arg(&a, 0), arg(&a, 1)).await
        }),
        ("BitgetExchange", "changePositionMode") => {
            async_member!(rt, name, |h, a| {
                rest_actions::change_position_mode(h, arg(&a, 0)).await
            })
        }
        ("BitgetExchange", "buildUtaStrategyPlaceOptsFromOrder") => {
            sync_member!(rt, name, |_h, a| {
                let order: Order = to_typed(&arg(&a, 0))?;
                from_typed(&order_logic::build_uta_strategy_place_opts_from_order(
                    &order,
                ))
            })
        }
        ("BitgetExchange", "getOrderSideV2") => sync_member!(rt, name, |_h, a| {
            let opts = arg(&a, 0);
            let side = side_of(&opts.get("side")?);
            let mode = mode_of(&arg(&a, 1));
            let reduce = if mode == PositionMode::Hedged {
                Some(opts.get("reduceOnly")?.truthy())
            } else {
                None
            };
            let (side, trade) = order_logic::get_order_side_v2(side, reduce, mode);
            Ok(Js::array(vec![
                enum_js(&side)?,
                trade.map_or(Js::Undefined, Js::str),
            ]))
        }),
        ("BitgetExchange", "getOrderSideUta") => sync_member!(rt, name, |_h, a| {
            let opts = arg(&a, 0);
            let side = side_of(&opts.get("side")?);
            let mode = mode_of(&arg(&a, 1));
            let reduce = if mode == PositionMode::Hedged {
                Some(opts.get("reduceOnly")?.truthy())
            } else {
                None
            };
            from_typed(&order_logic::get_order_side_uta(side, reduce, mode))
        }),
        ("BitgetExchange", "getUtaStrategyOrderSide") => sync_member!(rt, name, |_h, a| {
            let side = side_of(&arg(&a, 0).get("side")?);
            from_typed(&order_logic::get_uta_strategy_order_side(
                side,
                mode_of(&arg(&a, 1)),
            ))
        }),
        ("BitgetExchange", "getUtaStrategyTriggerBy") => sync_member!(rt, name, |_h, a| {
            let trigger = if arg(&a, 0).strict_equals(&Js::str("last_price")) {
                Some(OrderStopTriggerType::LastPrice)
            } else {
                None
            };
            Ok(Js::str(order_logic::get_uta_strategy_trigger_by(trigger)))
        }),
        ("BitgetExchange", "getAlgoOrderSide") => sync_member!(rt, name, |_h, a| {
            algo_order_side(&arg(&a, 0), &arg(&a, 1), order_logic::get_algo_order_side)
        }),
        ("BitgetInverseExchange", "getInverseAlgoOrderSide") => sync_member!(rt, name, |_h, a| {
            algo_order_side(
                &arg(&a, 0),
                &arg(&a, 1),
                inverse_logic::get_inverse_algo_order_side,
            )
        }),
        ("BitgetExchange", "isVirtualTPSLOrder") => sync_member!(rt, name, |_h, a| {
            let id = arg(&a, 0).get("id")?;
            match &id {
                Js::String(s) => Ok(Js::Bool(order_logic::is_virtual_tpsl_order(
                    &String::from_utf16_lossy(s),
                ))),
                Js::Undefined | Js::Null => Err(Thrown::type_error(&format!(
                    "Cannot read properties of {} (reading 'endsWith')",
                    id.to_display()
                ))),
                _ => Err(Thrown::type_error("order.id.endsWith is not a function")),
            }
        }),
        ("BitgetExchange", "isAlgoOrder") => sync_member!(rt, name, |_h, a| {
            let kind = order_type_of(&arg(&a, 0).get("type")?);
            Ok(Js::Bool(kind.is_some_and(order_logic::is_algo_order)))
        }),
        ("BitgetExchange", "getCancelPlanType") => {
            sync_member!(rt, name, |h, a| { cancel_plan_type(h, &arg(&a, 0)) })
        }
        ("BitgetExchange", "resolvePositionsForPlanType") => async_member!(rt, name, |h, a| {
            order_preparation::resolve_positions_for_plan_type(h, arg(&a, 0), arg(&a, 1)).await
        }),
        ("BitgetExchange", "planTypeComparableAmount") => sync_member!(rt, name, |h, a| {
            let opts = arg(&a, 0);
            let step = amount_step(h, &opts.get("symbol")?)?;
            let amount = opts.get("amount")?;
            Ok(match step {
                None => amount,
                step => Js::Number(order_logic::plan_type_comparable_amount(
                    amount.to_number()?,
                    step,
                )),
            })
        }),
        ("BitgetExchange", "floorToLot") => sync_member!(rt, name, |h, a| {
            let step = amount_step(h, &arg(&a, 0))?;
            let value = arg(&a, 1);
            Ok(match step {
                None => value,
                step => Js::Number(order_logic::floor_to_lot(value.to_number()?, step)),
            })
        }),
        ("BitgetExchange", "selectPlanTypeAndAmount") => {
            sync_member!(rt, name, |h, a| {
                select_plan_type_and_amount(h, &arg(&a, 0), &arg(&a, 1))
            })
        }
        ("BitgetExchange", "getOrderPlanTypeAndAmount") => {
            async_member!(rt, name, |h, a| {
                order_plan_type_and_amount(h, arg(&a, 0)).await
            })
        }
        ("BitgetPublicWebsocket", "listenOHLCV") => sync_member!(rt, name, |h, a| {
            runtime_orchestration::listen_ohlcv(h, arg(&a, 0), arg(&a, 1))
        }),
        ("BitgetPublicWebsocket", "listenOrderBook") => sync_member!(rt, name, |h, a| {
            runtime_orchestration::listen_order_book(h, arg(&a, 0), arg(&a, 1))
        }),
        _ => return Ok(None),
    };
    Ok(Some(f))
}
