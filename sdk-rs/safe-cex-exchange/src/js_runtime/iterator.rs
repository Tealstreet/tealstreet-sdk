//! Synchronous IteratorRecord operations over original native Js referents.
use super::value::{Js, Thrown};
use std::cell::Cell;

pub struct IteratorRecord {
    iterator: Js,
    next: Js,
    done: Cell<bool>,
}

impl IteratorRecord {
    pub fn get(value: &Js) -> Result<Self, Thrown> {
        Self::from_iterator(value.iterator()?)
    }
    /// Capture an already acquired native iterator without repeating GetMethod.
    pub fn from_iterator(iterator: Js) -> Result<Self, Thrown> {
        if !iterator.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        let next = iterator.get("next")?;
        Ok(Self {
            iterator,
            next,
            done: Cell::new(false),
        })
    }
    /// IteratorStep returns the actual result without reading its value (elisions).
    pub fn step(&self) -> Result<Option<Js>, Thrown> {
        if self.done.get() {
            return Ok(None);
        }
        let result = (|| {
            let step = self.next.call(self.iterator.clone(), vec![])?;
            if !step.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
            Ok((!step.get("done")?.truthy()).then_some(step))
        })();
        if !matches!(result, Ok(Some(_))) {
            self.done.set(true);
        }
        result
    }
    /// IteratorStepValue marks abrupt value access done without closing the iterator.
    pub fn step_value(&self) -> Result<Option<Js>, Thrown> {
        match self.step()? {
            None => Ok(None),
            Some(step) => match step.get("value") {
                Ok(value) => Ok(Some(value)),
                Err(error) => {
                    self.done.set(true);
                    Err(error)
                }
            },
        }
    }
    pub fn close(&self) -> Result<(), Thrown> {
        if self.done.replace(true) {
            return Ok(());
        }
        let method = self.iterator.get("return")?;
        if method.is_nullish() {
            return Ok(());
        }
        let result = method.call(self.iterator.clone(), vec![])?;
        if !result.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        Ok(())
    }
    pub fn close_after_throw(&self, error: Thrown) -> Thrown {
        // A throwing completion wins over every cleanup outcome after its effects.
        match self.close() {
            Ok(()) | Err(_) => error,
        }
    }
}
