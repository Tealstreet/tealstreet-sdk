//! Destructuring, spreads and parameter bindings with source IteratorClose.
use super::Host;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};

impl Host {
    /// `const [a, b, ...] = value` over `slots`; a `false` slot is an elision.
    /// After the last slot the iterator is closed unless already exhausted; a
    /// throwing `return` propagates (normal completion).
    pub(crate) fn array_pattern(&mut self, value: &Js, slots: &[bool]) -> Result<Vec<Js>, Thrown> {
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
                out.push(item.unwrap_or(Js::Undefined));
            }
        }
        if !done {
            self.iterator_close(&iterator)?;
        }
        Ok(out)
    }
    pub(crate) fn array_slots(&mut self, value: &Js, count: usize) -> Result<Vec<Js>, Thrown> {
        self.array_pattern(value, &vec![true; count])
    }
    /// `const [head, ...rest] = value`.
    pub(crate) fn head_rest(&mut self, value: &Js) -> Result<(Js, Js), Thrown> {
        let iterator = self.get_iterator(value)?;
        let head = self.iterator_step(&iterator)?.unwrap_or(Js::Undefined);
        let mut rest = vec![];
        while let Some(v) = self.iterator_step(&iterator)? {
            rest.push(v);
        }
        Ok((head, self.new_array(rest)))
    }
    /// `const { k1, k2 } = value`, with the source destructuring TypeError.
    pub(crate) fn object_pattern(&mut self, value: &Js, keys: &[&str]) -> Result<Vec<Js>, Thrown> {
        if value.is_nullish() {
            let first = keys.first().copied().unwrap_or("");
            let shown = if matches!(value, Js::Null) {
                "object null"
            } else {
                "undefined"
            };
            let what = if matches!(value, Js::Null) {
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
    pub(crate) fn copy_data_properties(&mut self, target: &Js, source: &Js) -> Result<(), Thrown> {
        source.copy_data_properties_to(target)
    }
    pub(crate) fn spread_object(&mut self, source: &Js) -> Result<Js, Thrown> {
        let out = self.new_object();
        self.copy_data_properties(&out, source)?;
        Ok(out)
    }
    /// `[...value]`.
    pub(crate) fn spread_to_array(&mut self, value: &Js) -> Result<Js, Thrown> {
        let items = self.collect_iterable(value)?;
        Ok(self.new_array(items))
    }
    /// `f(...value)` appends the iterable's items to an argument list.
    pub(crate) fn spread_args(&mut self, args: &mut Vec<Js>, value: &Js) -> Result<(), Thrown> {
        args.extend(self.collect_iterable(value)?);
        Ok(())
    }
    /// `[...array, last]`.
    pub(crate) fn spread_append(&mut self, iterable: &Js, last: &Js) -> Result<Js, Thrown> {
        let mut items = self.collect_iterable(iterable)?;
        items.push(last.clone());
        Ok(self.new_array(items))
    }
    /// Append to an array literal under construction (CreateDataProperty at its length).
    pub(crate) fn append(&mut self, array: &Js, value: &Js) -> Result<(), Thrown> {
        let len = self.get_str(array, "length")?;
        let len = self.to_number(&len)? as usize;
        self.define(array, Js::str(&len.to_string()), value.clone())
    }
    /// `const { data } = await response` with the source nullish diagnostic.
    pub(crate) fn data_binding(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(self.object_pattern(value, &["data"])?.remove(0))
    }
}
