//! Ordered live collection storage and the shared iterator protocol.
use super::value::{Js, NativeFn, Prop, Thrown};
use std::{cell::Cell, ops::Deref, rc::Rc};

/// Dense native access with stable insertion positions for suspended iterators.
/// Deleting an entry removes its position; reinsertion receives a new position.
pub struct Collection<T> {
    values: Vec<T>,
    positions: Vec<u64>,
    next_position: u64,
    pub(super) iterator_methods: std::cell::RefCell<
        std::collections::BTreeMap<&'static str, std::rc::Weak<super::value::FunctionData>>,
    >,
}

impl<T> Default for Collection<T> {
    fn default() -> Self {
        Self {
            values: vec![],
            positions: vec![],
            next_position: 0,
            iterator_methods: std::cell::RefCell::default(),
        }
    }
}

impl<T> Deref for Collection<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> Collection<T> {
    pub fn push(&mut self, value: T) {
        self.positions.push(self.next_position);
        self.next_position += 1;
        self.values.push(value);
    }
    pub fn clear(&mut self) {
        self.values.clear();
        self.positions.clear();
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        let mut retained = vec![];
        self.values.retain(|value| {
            let keep = keep(value);
            retained.push(keep);
            keep
        });
        let mut retained = retained.into_iter();
        self.positions
            .retain(|_| retained.next().expect("one position per collection entry"));
    }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.values.iter_mut()
    }

    pub(super) fn following(&self, position: u64) -> Option<(u64, &T)> {
        let index = self.positions.partition_point(|at| *at < position);
        self.positions
            .get(index)
            .map(|at| (*at, &self.values[index]))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Key,
    Value,
    Entry,
}

pub(crate) fn collection_iterator(collection: Js, kind: Kind) -> Js {
    let position = Cell::new(0);
    let done = Cell::new(false);
    let tag = if matches!(collection, Js::Map(_)) {
        "Map Iterator"
    } else {
        "Set Iterator"
    };
    let collection = std::cell::RefCell::new(Some(collection));
    let next: NativeFn = Rc::new(move |_, _| {
        let active = collection.borrow().clone();
        let item = if done.get() {
            None
        } else {
            match active
                .as_ref()
                .expect("non-terminal iterator retains collection")
            {
                Js::Map(entries) => {
                    entries
                        .borrow()
                        .following(position.get())
                        .map(|(at, (key, value))| {
                            position.set(at + 1);
                            match kind {
                                Kind::Key => key.clone(),
                                Kind::Value => value.clone(),
                                Kind::Entry => Js::array(vec![key.clone(), value.clone()]),
                            }
                        })
                }
                Js::Set(entries) => {
                    entries
                        .borrow()
                        .following(position.get())
                        .map(|(at, value)| {
                            position.set(at + 1);
                            match kind {
                                Kind::Entry => Js::array(vec![value.clone(), value.clone()]),
                                _ => value.clone(),
                            }
                        })
                }
                _ => unreachable!("collection iterator requires Map or Set"),
            }
        };
        if item.is_none() {
            done.set(true);
            collection.borrow_mut().take();
        }
        Ok(Js::object(vec![
            ("value", item.clone().unwrap_or(Js::Undefined)),
            ("done", Js::Bool(item.is_none())),
        ]))
    });
    iterator_object(tag, next)
}

fn iterator_object(tag: &'static str, next: NativeFn) -> Js {
    let object = Js::object_with_class(tag, vec![]);
    if let Js::Object(data) = &object {
        *data.iterator_next.borrow_mut() = Some(next);
    }
    object
}

pub(crate) fn iterator_next_method(brand: &'static str) -> Js {
    Js::function("next", move |this, args| {
        let next = if let Js::Object(data) = &this {
            if data.class == brand {
                data.iterator_next.borrow().clone()
            } else {
                None
            }
        } else {
            None
        };
        let next = next
            .ok_or_else(|| Thrown::type_error("Iterator next called on incompatible receiver"))?;
        next(this, args)
    })
}

pub(crate) fn iterator_method(value: &Js) -> Result<Js, Thrown> {
    iterator_method_receiver(value, value)
}
fn iterator_method_receiver(value: &Js, receiver: &Js) -> Result<Js, Thrown> {
    if let Some(slot) = value.symbol_iterator_slot() {
        let property = slot.borrow().clone();
        if let Some(property) = property {
            return match property {
                Prop::Data(value) | Prop::DataDescriptor { value, .. } => Ok(value),
                Prop::Accessor { get: Some(get), .. } => get.call(receiver.clone(), vec![]),
                Prop::Accessor { get: None, .. } => Ok(Js::Undefined),
            };
        }
        if let Js::Object(data) = value
            && data.iterator_next.borrow().is_some()
        {
            return Ok(Js::function("[Symbol.iterator]", |this, _| Ok(this)));
        }
        if let Some(proto) = value.prototype() {
            return iterator_method_receiver(&proto, receiver);
        }
        return Ok(Js::Undefined);
    }
    match value {
        Js::Map(_) => value.get("entries"),
        Js::Set(_) => value.get("values"),
        Js::String(_) => Ok(array_iterator_method()),
        _ => Ok(Js::Undefined),
    }
}

#[cfg(test)]
#[path = "collection_contract_tests.rs"]
pub(super) mod accepted_contract_tests;

pub(crate) fn array_iterator_method() -> Js {
    Js::function("values", |this, _| {
        if !matches!(this, Js::Array(_) | Js::String(_)) {
            return Err(Thrown::type_error("value is not iterable"));
        }
        let tag = if matches!(this, Js::String(_)) {
            "String Iterator"
        } else {
            "Array Iterator"
        };
        let index = Cell::new(0usize);
        let done = Cell::new(false);
        let next: NativeFn = Rc::new(move |_, _| {
            let at = index.get();
            let item = if done.get() {
                None
            } else {
                match &this {
                    Js::Array(items) if at < items.len() => Some(this.get(&at.to_string())?),
                    Js::String(units) if at < units.len() => {
                        let pair = (0xD800..=0xDBFF).contains(&units[at])
                            && units
                                .get(at + 1)
                                .is_some_and(|u| (0xDC00..=0xDFFF).contains(u));
                        let width = if pair { 2 } else { 1 };
                        index.set(at + width - 1);
                        Some(Js::utf16(&units[at..at + width]))
                    }
                    _ => None,
                }
            };
            if item.is_some() {
                index.set(index.get() + 1);
            } else {
                done.set(true);
            }
            Ok(Js::object(vec![
                ("value", item.clone().unwrap_or(Js::Undefined)),
                ("done", Js::Bool(item.is_none())),
            ]))
        });
        Ok(iterator_object(tag, next))
    })
}
