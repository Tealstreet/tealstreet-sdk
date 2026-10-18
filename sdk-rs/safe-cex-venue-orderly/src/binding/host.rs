use super::{Rt, Shared, scope, text};
use crate::{
    account_order_reads::AccountReadHost,
    data_mappers::{DataHost, Op},
    ws_controls::WsHost,
};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown},
};
use std::rc::{Rc, Weak};
pub(super) type V = Rc<Js>;
pub(super) type R<T> = Result<T, Thrown>;
pub(super) fn v(value: Js) -> V {
    Rc::new(value)
}
#[derive(Clone)]
pub(super) struct Host {
    pub weak: Weak<Shared>,
    pub this: Js,
}
impl Host {
    pub(super) fn new(rt: &Rt) -> Self {
        Self {
            weak: Rc::downgrade(&rt.shared),
            this: rt.this.clone(),
        }
    }
    pub(super) fn rt(&self) -> R<Rt> {
        scope(&self.weak, self.this.clone())
    }
    pub(super) fn global(&self, name: &str, args: Vec<V>) -> R<V> {
        Ok(v(self.rt()?.global(name)?.call(
            Js::Undefined,
            args.into_iter().map(|x| (*x).clone()).collect(),
        )?))
    }
    pub(super) fn values(&self, rows: &Js) -> R<Vec<Js>> {
        rows.array_values()
            .ok_or_else(|| Thrown::type_error("Expected an array"))
    }
    pub(super) fn function(&self, name: &str, body: crate::classes::BoundMember<Self>) -> V {
        let weak = self.weak.clone();
        let this = self.this.clone();
        v(Js::function(name, move |_, args| {
            let rt = scope(&weak, this.clone())?;
            match body(args.into_iter().map(v).collect())? {
                crate::classes::Return::Value(value) => Ok((*value).clone()),
                crate::classes::Return::Future(future) => {
                    Ok(rt.spawn(Box::pin(async move { Ok((*future.await?).clone()) })))
                }
            }
        }))
    }
}
impl WsHost for Host {
    type Value = Js;
    type Error = Thrown;
    type Iterator = (V, usize);
    fn get(&self, value: &V, key: &[u16]) -> R<V> {
        Ok(v(value.get_key(key)?))
    }
    fn set(&self, value: &V, key: &[u16], next: V) -> R<()> {
        value.set_key(key, (*next).clone())
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn string(&self, value: &Js) -> R<Vec<u16>> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn text(&self, value: &[u16]) -> V {
        v(Js::utf16(value))
    }
    fn object(&self, entries: Vec<(&'static str, V)>) -> R<V> {
        Ok(v(Js::object(
            entries
                .into_iter()
                .map(|(k, v)| (k, (*v).clone()))
                .collect(),
        )))
    }
    fn stringify(&self, value: V) -> R<V> {
        self.global("JSON.stringify", vec![value])
    }
    fn parse(&self, value: V) -> R<V> {
        self.global("JSON.parse", vec![value])
    }
    fn includes(&self, value: &V, needle: &[u16]) -> R<bool> {
        Ok(value
            .call_method("includes", vec![Js::utf16(needle)])?
            .truthy())
    }
    fn own_values(&self, value: V) -> R<Vec<V>> {
        Ok(value
            .own_entries()?
            .into_iter()
            .map(|(_, x)| v(x))
            .collect())
    }
    fn flatten_one(&self, values: Vec<V>) -> R<Vec<V>> {
        let mut out = vec![];
        for value in values {
            if let Some(items) = value.array_values() {
                out.extend(items.into_iter().map(v));
            } else {
                out.push(value);
            }
        }
        Ok(out)
    }
    fn entries(&self, value: V) -> R<Vec<(Vec<u16>, V)>> {
        Ok(value
            .own_entries()?
            .into_iter()
            .map(|(k, x)| (k.to_vec(), v(x)))
            .collect())
    }
    fn iterator(&self, value: V) -> R<Self::Iterator> {
        self.values(&value)?;
        Ok((value, 0))
    }
    fn next(&self, iterator: &mut Self::Iterator) -> R<Option<V>> {
        let len = iterator.0.get("length")?.to_number()? as usize;
        if iterator.1 >= len {
            return Ok(None);
        }
        let value = iterator.0.get(&iterator.1.to_string())?;
        iterator.1 += 1;
        Ok(Some(v(value)))
    }
    fn call(&self, function: V, receiver: Option<V>, args: Vec<V>) -> R<V> {
        Ok(v(function.call(
            receiver.map_or(Js::Undefined, |x| (*x).clone()),
            args.into_iter().map(|x| (*x).clone()).collect(),
        )?))
    }
    fn monotonic_now(&self) -> R<V> {
        Ok(v(Js::Number(self.rt()?.shared.runtime.monotonic_now_ms())))
    }
    fn virtual_now(&self) -> R<V> {
        self.global("virtualClock.getCurrentTimeMs", vec![])
    }
    fn sign(&self, message: V, secret: V) -> R<V> {
        let result = crate::encryption::orderly_sign(&text(&message)?, &text(&secret)?)
            .map_err(|e| Thrown::error(e.name, &e.message))?;
        Ok(v(Js::str(&result)))
    }
    fn reconnecting(&self) -> bool {
        self.rt()
            .is_ok_and(|rt| rt.shared.runtime.enable_reconnecting())
    }
}
impl AccountReadHost for Host {
    async fn await_value(&self, value: V) -> R<V> {
        Ok(v(await_js(&self.rt()?.exchange_runtime(), (*value).clone()).await?))
    }
    fn error_value(&self, error: &Thrown) -> V {
        v(error.value.clone())
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn is_null(&self, value: &Js) -> bool {
        matches!(value, Js::Null)
    }
    fn is_string(&self, value: &Js) -> bool {
        value.is_string()
    }
    fn text_eq(&self, value: &Js, expected: &str) -> bool {
        value.strict_equals(&Js::str(expected))
    }
    fn undefined(&self) -> V {
        v(Js::Undefined)
    }
    fn null(&self) -> V {
        v(Js::Null)
    }
    fn number(&self, value: f64) -> V {
        v(Js::Number(value))
    }
    fn array(&self, values: Vec<V>) -> V {
        v(Js::array(
            values.into_iter().map(|x| (*x).clone()).collect(),
        ))
    }
    fn is_array(&self, value: &Js) -> bool {
        matches!(value, Js::Array(_))
    }
    fn spread(&self, value: V) -> R<Vec<V>> {
        Ok(self.values(&value)?.into_iter().map(v).collect())
    }
    fn reduce(&self, rows: V, mut initial: V, mut callback: impl FnMut(V, V) -> R<V>) -> R<V> {
        for row in self.values(&rows)? {
            initial = callback(initial, v(row))?;
        }
        Ok(initial)
    }
    fn normalize_symbol(&self, value: V) -> R<V> {
        Ok(v(Js::str(&crate::utils::normalize_symbol(&text(&value)?))))
    }
    fn get_key(&self, value: V, key: &str) -> R<V> {
        super::primitives::get_key(&value, key)
    }
}
impl DataHost for Host {
    fn numeric(&self, value: V) -> R<f64> {
        value.to_number()
    }
    fn arithmetic(&self, op: Op, left: V, right: V) -> R<V> {
        use safe_cex_exchange::js_runtime::ops;
        Ok(v(match op {
            Op::Sub => ops::subtract(&left, &right)?,
            Op::Mul => ops::multiply(&left, &right)?,
            Op::Div => ops::divide(&left, &right)?,
            Op::Min => (*self.global("Math.min", vec![left, right])?).clone(),
            Op::Max => (*self.global("Math.max", vec![left, right])?).clone(),
        }))
    }
    fn compare(&self, left: V, right: V, less: bool) -> R<bool> {
        if less {
            safe_cex_exchange::js_runtime::ops::less_than(&left, &right)
        } else {
            safe_cex_exchange::js_runtime::ops::greater_than(&left, &right)
        }
    }
    fn round(&self, value: V) -> R<V> {
        Ok(v(Js::Number(safe_cex_num::math_round(value.to_number()?))))
    }
    fn split(&self, value: V) -> R<Vec<V>> {
        Ok(self
            .values(&value.call_method("split", vec![Js::str("_")])?)?
            .into_iter()
            .map(v)
            .collect())
    }
    fn starts_with(&self, value: V, prefix: &str) -> R<bool> {
        Ok(value
            .call_method("startsWith", vec![Js::str(prefix)])?
            .truthy())
    }
    fn filter(&self, rows: V, mut predicate: impl FnMut(V) -> R<bool>) -> R<V> {
        let mut out = vec![];
        for row in self.values(&rows)? {
            let row = v(row);
            if predicate(row.clone())? {
                out.push(row);
            }
        }
        Ok(self.array(out))
    }
    fn map(&self, rows: V, mut callback: impl FnMut(V) -> R<V>) -> R<V> {
        Ok(self.array(
            self.values(&rows)?
                .into_iter()
                .map(|x| callback(v(x)))
                .collect::<R<_>>()?,
        ))
    }
    fn clone_deep(&self, value: V) -> R<V> {
        Ok(v(super::primitives::deep_clone(&value)?))
    }
    fn merge(&self, value: V, fields: Vec<(&'static str, V)>) -> R<V> {
        let out = value.spread_object()?;
        for (key, value) in fields {
            out.set(key, (*value).clone())?;
        }
        Ok(v(out))
    }
    fn uniq_id(&self, value: V) -> R<V> {
        self.global("uniqBy", vec![value, v(Js::str("id"))])
    }
    fn reverse(&self, value: V) -> R<V> {
        Ok(v(value.call_method("reverse", vec![])?))
    }
    fn push(&self, value: V, items: Vec<V>) -> R<()> {
        for item in items {
            value.push((*item).clone())?;
        }
        Ok(())
    }
    fn reverse_symbol(&self, value: V) -> R<V> {
        Ok(v(Js::str(&crate::utils::reverse_symbol(&text(&value)?))))
    }
    fn order_type(&self, value: V) -> R<V> {
        super::primitives::table(&crate::types::ORDER_TYPE, &value, false)
    }
    fn order_side(&self, value: V) -> R<V> {
        super::primitives::table(&crate::types::ORDER_SIDE, &value, false)
    }
    fn subtract(&self, left: V, right: V) -> R<V> {
        self.global("subtract", vec![left, right])
    }
    fn is_undefined(&self, value: &Js) -> bool {
        value.is_undefined()
    }
    fn boolean(&self, value: bool) -> V {
        v(Js::Bool(value))
    }
}
