//! Destructuring, spreads and parameter bindings with source IteratorClose.
use super::{
    Realm,
    value::{PropKey, Thrown, Value},
};

impl Realm {
    /// `const [a, b, ...] = value` over `slots`; a `false` slot is an elision.
    /// After the last slot the iterator is closed unless already exhausted; a
    /// throwing `return` propagates (normal completion).
    pub(crate) fn array_pattern(
        &mut self,
        value: &Value,
        slots: &[bool],
    ) -> Result<Vec<Value>, Thrown> {
        let iterator = self.get_iterator(value)?;
        let mut out = vec![];
        let mut done = false;
        for &bound in slots {
            let item = if done {
                None
            } else {
                let step = self.iterator_step(&iterator)?;
                done = step.is_none();
                step
            };
            if bound {
                out.push(item.unwrap_or(Value::Undefined));
            }
        }
        if !done {
            self.iterator_close(&iterator)?;
        }
        Ok(out)
    }
    pub(crate) fn array_slots(
        &mut self,
        value: &Value,
        count: usize,
    ) -> Result<Vec<Value>, Thrown> {
        self.array_pattern(value, &vec![true; count])
    }
    /// `const [head, ...rest] = value`.
    pub(crate) fn head_rest(&mut self, value: &Value) -> Result<(Value, Value), Thrown> {
        let iterator = self.get_iterator(value)?;
        let head = self.iterator_step(&iterator)?.unwrap_or(Value::Undefined);
        let mut rest = vec![];
        while let Some(v) = self.iterator_step(&iterator)? {
            rest.push(v);
        }
        Ok((head, self.new_array(rest)))
    }
    /// `const { k1, k2 } = value`, with the source destructuring TypeError.
    pub(crate) fn object_pattern(
        &mut self,
        value: &Value,
        keys: &[&str],
    ) -> Result<Vec<Value>, Thrown> {
        if value.is_nullish() {
            let first = keys.first().copied().unwrap_or("");
            let shown = if matches!(value, Value::Null) {
                "object null"
            } else {
                "undefined"
            };
            let what = if matches!(value, Value::Null) {
                "null"
            } else {
                "undefined"
            };
            return Err(self.type_error(&format!(
                "Cannot destructure property '{first}' of '{shown}' as it is {what}."
            )));
        }
        keys.iter().map(|k| self.get_str(value, k)).collect()
    }
    /// Object spread / CopyDataProperties: own enumerable string then symbol keys.
    pub(crate) fn copy_data_properties(
        &mut self,
        target: &Value,
        source: &Value,
    ) -> Result<(), Thrown> {
        if source.is_nullish() {
            return Ok(());
        }
        if let Value::String(u) = source {
            for (i, unit) in u.iter().enumerate() {
                self.define(target, PropKey::str(&i.to_string()), Value::units(&[*unit]))?;
            }
            return Ok(());
        }
        let mut keys = self.own_keys(source, true);
        if let Some(o) = source.object() {
            for p in &o.borrow().props {
                if p.enumerable && matches!(p.key, PropKey::Symbol(_)) {
                    keys.push(p.key.clone());
                }
            }
        }
        for key in keys {
            let v = self.get(source, &key)?;
            self.define(target, key, v)?;
        }
        Ok(())
    }
    pub(crate) fn spread_object(&mut self, source: &Value) -> Result<Value, Thrown> {
        let out = self.new_object();
        self.copy_data_properties(&out, source)?;
        Ok(out)
    }
    /// `[...value]`.
    pub(crate) fn spread_to_array(&mut self, value: &Value) -> Result<Value, Thrown> {
        let items = self.collect_iterable(value)?;
        Ok(self.new_array(items))
    }
    /// `f(...value)` appends the iterable's items to an argument list.
    pub(crate) fn spread_args(
        &mut self,
        args: &mut Vec<Value>,
        value: &Value,
    ) -> Result<(), Thrown> {
        args.extend(self.collect_iterable(value)?);
        Ok(())
    }
    /// `[...array, last]`.
    pub(crate) fn spread_append(
        &mut self,
        iterable: &Value,
        last: &Value,
    ) -> Result<Value, Thrown> {
        let mut items = self.collect_iterable(iterable)?;
        items.push(last.clone());
        Ok(self.new_array(items))
    }
    /// Append to an array literal under construction (CreateDataProperty at its length).
    pub(crate) fn append(&mut self, array: &Value, value: &Value) -> Result<(), Thrown> {
        let len = self.get_str(array, "length")?;
        let len = self.to_number(&len)? as usize;
        self.define(array, PropKey::str(&len.to_string()), value.clone())
    }
    /// `const { data } = await response` with the source nullish diagnostic.
    pub(crate) fn data_binding(&mut self, value: &Value) -> Result<Value, Thrown> {
        Ok(self.object_pattern(value, &["data"])?.remove(0))
    }
    /// for-in: enumerable string keys along the prototype chain, own first,
    /// skipping shadowed names. Snapshotted at loop entry.
    pub(crate) fn for_in_keys(&mut self, value: &Value) -> Vec<Value> {
        let mut seen: Vec<PropKey> = vec![];
        let mut out = vec![];
        let mut current = value.object().cloned();
        if let Value::String(_) = value {
            return self
                .own_keys(value, true)
                .into_iter()
                .map(|k| k.to_value())
                .collect();
        }
        while let Some(o) = current {
            let v = Value::Object(o.clone());
            let all = self.own_keys(&v, false);
            let enumerable = self.own_keys(&v, true);
            for k in all {
                if seen.iter().any(|s| s.same(&k)) {
                    continue;
                }
                if enumerable.iter().any(|e| e.same(&k)) {
                    out.push(k.to_value());
                }
                seen.push(k);
            }
            current = o.borrow().proto.clone();
        }
        out
    }
    /// `super.name` from a method whose home object's prototype is `home_proto`.
    pub(crate) fn super_get(
        &mut self,
        home_proto: &Value,
        receiver: &Value,
        name: &str,
    ) -> Result<Value, Thrown> {
        let parent = home_proto
            .object()
            .and_then(|o| o.borrow().proto.clone())
            .map(Value::Object)
            .unwrap_or(Value::Undefined);
        if parent.is_nullish() {
            return Ok(Value::Undefined);
        }
        self.get_with_receiver(&parent, &PropKey::str(name), receiver)
    }
}
