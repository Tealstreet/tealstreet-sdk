//! Language and imported operations, retaining native JS handles across awaits.
use super::Rt;
use crate::{
    collection_reads::{Callback, Op, ReadHost},
    runtime_lifecycle::LifecycleHost,
    trading_rest::{AsyncCallback, TradingHost},
};
use safe_cex_exchange::js_runtime::{
    ops,
    promise::{Promise, await_js},
    value::{Js, Thrown},
};

pub(super) fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()?
        .ok_or_else(|| Thrown::type_error("value is not iterable"))
}
pub(super) fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}
pub(super) fn find(
    v: Js,
    mut predicate: impl FnMut(Js) -> Result<bool, Thrown>,
) -> Result<Js, Thrown> {
    for value in items(&v)? {
        if predicate(value.clone())? {
            return Ok(value);
        }
    }
    Ok(Js::Undefined)
}
pub(super) fn global_call(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    rt.global(name)?.call(Js::Undefined, args)
}
impl ReadHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn text(&self, s: &str) -> Js {
        Js::str(s)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn get(&self, v: &Js, key: &str) -> Result<Js, Thrown> {
        v.get(key)
    }
    fn set(&self, v: &Js, key: &str, next: Js) -> Result<(), Thrown> {
        v.set(key, next)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn set_equal(&self, a: &Js, b: &Js) -> bool {
        a.same_value_zero(b)
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn is_boolean(&self, v: &Js) -> bool {
        matches!(v, Js::Bool(_))
    }
    fn is_finite(&self, v: &Js) -> bool {
        matches!(v, Js::Number(n) if n.is_finite())
    }
    fn numeric(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.to_number()?))
    }
    fn string(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::String(v.to_js_string()?))
    }
    fn uppercase(&self, v: Js) -> Result<Js, Thrown> {
        v.call_method("toUpperCase", vec![])
    }
    fn concat(&self, v: Vec<Js>) -> Result<Js, Thrown> {
        let mut units = vec![];
        for part in v {
            units.extend_from_slice(&part.to_js_string()?);
        }
        Ok(Js::utf16(&units))
    }
    fn arithmetic(&self, op: Op, a: Js, b: Js) -> Result<Js, Thrown> {
        match op {
            Op::Add => ops::add(&a, &b),
            Op::Sub => ops::subtract(&a, &b),
            Op::Mul => ops::multiply(&a, &b),
            Op::Div => ops::divide(&a, &b),
            Op::Min => Ok(Js::Number(safe_cex_num::math_min(
                a.to_number()?,
                b.to_number()?,
            ))),
            Op::Max => Ok(Js::Number(safe_cex_num::math_max(
                a.to_number()?,
                b.to_number()?,
            ))),
        }
    }
    fn less(&self, a: Js, b: Js) -> Result<bool, Thrown> {
        ops::less_than(&a, &b)
    }
    fn abs(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.to_number()?.abs()))
    }
    fn floor(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.to_number()?.floor()))
    }
    fn date_parse(&self, v: Js) -> Result<Js, Thrown> {
        global_call(self, "Date.parse", vec![v])
    }
    fn split_at(&self, v: Js, separator: &str, index: usize) -> Result<Js, Thrown> {
        v.call_method("split", vec![Js::str(separator)])?
            .get(&index.to_string())
    }
    fn array(&self, v: Vec<Js>) -> Js {
        Js::array(v)
    }
    fn object(&self, v: Vec<(&'static str, Js)>) -> Js {
        Js::object(v)
    }
    fn assign_spread(&self, target: Js, value: Js) -> Result<(), Thrown> {
        if !value.is_nullish() {
            for (k, v) in value.own_entries()? {
                target.define_key(&k, v)?;
            }
        }
        Ok(())
    }
    fn omit_undefined(&self, v: Js) -> Result<Js, Thrown> {
        global_call(self, "omitUndefined", vec![v])
    }
    fn items(&self, v: Js) -> Result<Vec<Js>, Thrown> {
        v.iterable_values()
    }
    fn map(&self, v: Js, f: impl FnMut(Js) -> Result<Js, Thrown>) -> Result<Js, Thrown> {
        Ok(Js::array(
            items(&v)?.into_iter().map(f).collect::<Result<_, _>>()?,
        ))
    }
    fn filter(&self, v: Js, mut f: impl FnMut(Js) -> Result<bool, Thrown>) -> Result<Js, Thrown> {
        let mut out = vec![];
        for x in items(&v)? {
            if f(x.clone())? {
                out.push(x)
            }
        }
        Ok(Js::array(out))
    }
    fn some(&self, v: Js, mut f: impl FnMut(Js) -> Result<bool, Thrown>) -> Result<bool, Thrown> {
        for x in items(&v)? {
            if f(x)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn includes(&self, v: Js, needle: Js) -> Result<bool, Thrown> {
        Ok(v.call_method("includes", vec![needle])?.truthy())
    }
    fn flatten(&self, v: Js) -> Result<Js, Thrown> {
        v.call_method("flat", vec![])
    }
    fn sort_time_desc(&self, v: Js) -> Result<Js, Thrown> {
        let comparator = Js::function("", |_, a| {
            ops::subtract(&a[1].get("timestamp")?, &a[0].get("timestamp")?)
        });
        v.call_method("sort", vec![comparator])
    }
    fn slice_limit(&self, v: Js, limit: Js) -> Result<Js, Thrown> {
        v.call_method("slice", vec![Js::Number(0.), limit])
    }
    fn table(&self, name: &str, key: Js) -> Result<Js, Thrown> {
        self.global(name)?.get_key(&key.to_js_string()?)
    }
    fn call(&self, f: Js, receiver: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        f.call(receiver, args)
    }
    fn function(&self, f: Callback<Self>) -> Js {
        Js::function("", move |_, a| f(a))
    }
    fn map_series(&self, values: Js, callback: Js) -> Result<Js, Thrown> {
        global_call(self, "mapSeries", vec![values, callback])
    }
    async fn await_value(&self, v: Js) -> Result<Js, Thrown> {
        await_js(&self.exchange_runtime(), v).await
    }
    fn observe_detached(&self, _v: Js) {}
    fn sleep(&self, ms: f64) -> Result<Js, Thrown> {
        global_call(self, "sleep", vec![Js::Number(ms)])
    }
    fn console(&self, method: &str, message: &str) -> Result<(), Thrown> {
        global_call(self, &format!("console.{method}"), vec![Js::str(message)])?;
        Ok(())
    }
    fn error(&self, message: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::new(
            self.global("Error")?.call(Js::Undefined, vec![message])?,
        ))
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    fn position_id(&self, symbol: Js, side: Js, mode: Js) -> Result<Js, Thrown> {
        global_call(self, "createPositionId", vec![symbol, side, mode])
    }
}
impl LifecycleHost for Rt {
    fn builtin(&self, name: &'static str) -> Result<Js, Thrown> {
        self.global(name)
    }
    fn dynamic(&self, value: Js, key: Js) -> Result<Js, Thrown> {
        value.get_key(&key.to_js_string()?)
    }
    fn set_add(&self, set: Js, value: Js) -> Result<(), Thrown> {
        set.call_method("add", vec![value])?;
        Ok(())
    }
    fn set_clear(&self, set: Js) -> Result<(), Thrown> {
        set.call_method("clear", vec![])?;
        Ok(())
    }
    fn set_values(&self, set: Js) -> Result<Js, Thrown> {
        let Js::Set(values) = set else {
            return Err(Thrown::type_error("not a Set"));
        };
        Ok(Js::array(values.borrow().clone()))
    }
    fn set_size(&self, set: Js) -> Result<Js, Thrown> {
        set.get("size")
    }
    fn find(
        &self,
        array: Js,
        predicate: impl FnMut(Js) -> Result<bool, Thrown>,
    ) -> Result<Js, Thrown> {
        find(array, predicate)
    }
    fn super_dispose(&self, receiver: Js) -> Result<Js, Thrown> {
        super::classes::super_member(self, &receiver, "dispose")?.call(receiver, vec![])
    }
}
impl TradingHost for Rt {
    fn key(&self, v: &Js, key: Js) -> Result<Js, Thrown> {
        v.get_key(&key.to_js_string()?)
    }
    fn set_dynamic(&self, v: &Js, key: Js, value: Js) -> Result<(), Thrown> {
        v.set_key(&key.to_js_string()?, value)
    }
    fn null_is(&self, v: &Js) -> bool {
        matches!(v, Js::Null)
    }
    fn has(&self, v: Js, key: &str) -> Result<bool, Thrown> {
        if !v.is_object() {
            return Err(Thrown::type_error("right-hand side of in is not an object"));
        }
        let mut value = v;
        loop {
            if value.has_own(key) || (value.is_array() && key == "length") {
                return Ok(true);
            }
            let next = match &value {
                Js::Object(o) => o.proto.borrow().clone(),
                _ => None,
            };
            match next {
                Some(parent) => value = parent,
                None => return Ok(false),
            }
        }
    }
    fn flat_map(&self, v: Js, mut f: impl FnMut(Js) -> Result<Js, Thrown>) -> Result<Js, Thrown> {
        let mut out = vec![];
        for x in items(&v)? {
            let value = f(x)?;
            if value.is_array() {
                out.extend(items(&value)?)
            } else {
                out.push(value)
            }
        }
        Ok(Js::array(out))
    }
    fn find(&self, v: Js, f: impl FnMut(Js) -> Result<bool, Thrown>) -> Result<Js, Thrown> {
        find(v, f)
    }
    fn copy(&self, v: Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn remove_key(&self, v: Js, key: &str) -> Result<(), Thrown> {
        v.delete(&key.encode_utf16().collect::<Vec<_>>())?;
        Ok(())
    }
    fn chunk(&self, v: Js, n: usize) -> Result<Js, Thrown> {
        global_call(self, "chunk", vec![v, Js::Number(n as f64)])
    }
    fn times(&self, n: Js, mut f: impl FnMut() -> Result<Js, Thrown>) -> Result<Js, Thrown> {
        let mut out = vec![];
        // lodash times returns [] above MAX_SAFE_INTEGER (including infinity),
        // rather than attempting a saturated usize allocation/loop.
        let count = n.to_number()?.trunc();
        if !(1.0..=9_007_199_254_740_991.0).contains(&count) {
            return Ok(Js::array(out));
        }
        for _ in 0..(count as usize) {
            out.push(f()?)
        }
        Ok(Js::array(out))
    }
    fn push(&self, v: Js, item: Js) -> Result<(), Thrown> {
        v.push(item)
    }
    fn ceil(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.to_number()?.ceil()))
    }
    fn remainder(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::remainder(&a, &b)
    }
    fn power(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        global_call(self, "Math.pow", vec![a, b])
    }
    fn adjust(&self, v: Js, step: Js) -> Result<Js, Thrown> {
        global_call(self, "adjust", vec![v, step])
    }
    fn parse_float(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.parse_float()?))
    }
    fn uuid(&self) -> Result<Js, Thrown> {
        global_call(self, "uuid", vec![])
    }
    fn inverse_order_type(&self, key: Js) -> Result<Js, Thrown> {
        global_call(self, "inverseObj", vec![self.global("ORDER_TYPE")?])?
            .get_key(&key.to_js_string()?)
    }
    fn now(&self) -> Result<Js, Thrown> {
        Ok(Js::Number(self.shared.runtime.now_ms()))
    }
    fn async_function(&self, f: AsyncCallback<Self>) -> Js {
        let runtime = self.exchange_runtime();
        Js::function("", move |_, a| {
            Ok(Js::Promise(Promise::spawn(&runtime, f(a))))
        })
    }
    fn promise_all(&self, promises: Js) -> Result<Js, Thrown> {
        global_call(self, "Promise.all", vec![promises])
    }
}
