//! One sparse array owner: descriptors, virtual length, symbol and prototype.
use super::{Js, Prop, Str, Thrown, array_index, utf16};
use indexmap::IndexMap;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub struct ArrayData {
    pub(super) props: RefCell<IndexMap<Str, Prop>>,
    pub(super) length: Cell<u32>,
    pub(super) length_writable: Cell<bool>,
    pub(super) symbol_iterator: RefCell<Option<Prop>>,
    pub(super) proto: RefCell<Option<Js>>,
    pub(super) extensible: Cell<bool>,
}

impl ArrayData {
    pub(super) fn new(items: Vec<Option<Js>>) -> Self {
        let length = u32::try_from(items.len()).expect("native array length fits uint32");
        Self {
            props: RefCell::new(
                items
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.map(|v| (utf16(&i.to_string()), Prop::Data(v))))
                    .collect(),
            ),
            length: Cell::new(length),
            length_writable: Cell::new(true),
            symbol_iterator: RefCell::new(None),
            proto: RefCell::new(Some(prototype())),
            extensible: Cell::new(true),
        }
    }
    pub fn len(&self) -> usize {
        self.length.get() as usize
    }
    pub fn is_empty(&self) -> bool {
        self.length.get() == 0
    }
}

pub(in crate::js_runtime) fn prototype() -> Js {
    thread_local! {
        // Realm prototype roots, not per-array value/descriptor mirrors.
        static PROTOTYPES: RefCell<Vec<(Weak<safe_cex_num::PrototypeRealm>, Weak<ArrayData>)>> = const { RefCell::new(Vec::new()) };
    }

    let realm = safe_cex_num::PrototypeRealm::current();
    if let Some(array) = PROTOTYPES.with(|roots| {
        roots.borrow().iter().find_map(|(r, a)| {
            (r.upgrade().is_some_and(|r| Rc::ptr_eq(&r, &realm)))
                .then(|| a.upgrade())
                .flatten()
        })
    }) {
        return Js::Array(array);
    }
    let array = Rc::new(ArrayData {
        props: RefCell::new(IndexMap::new()),
        length: Cell::new(0),
        length_writable: Cell::new(true),
        symbol_iterator: RefCell::new(Some(Prop::DataDescriptor {
            value: super::super::collections::array_iterator_method(),
            writable: true,
            enumerable: false,
            configurable: true,
        })),
        proto: RefCell::new(Some(Js::Builtin(
            realm.reference(safe_cex_num::ProtoMember::Prototype),
        ))),
        extensible: Cell::new(true),
    });
    for name in [
        "push",
        "pop",
        "reverse",
        "includes",
        "indexOf",
        "join",
        "slice",
        "concat",
        "map",
        "forEach",
        "filter",
        "some",
        "every",
        "find",
        "findIndex",
        "reduce",
        "flat",
        "flatMap",
        "sort",
    ] {
        array.props.borrow_mut().insert(
            utf16(name),
            Prop::DataDescriptor {
                value: Js::function(name, move |this, args| {
                    super::super::builtins::array_member(this.clone(), name)
                        .expect("registered array method")
                        .call(this, args)
                }),
                writable: true,
                enumerable: false,
                configurable: true,
            },
        );
    }
    array.props.borrow_mut().insert(
        utf16("toString"),
        Prop::DataDescriptor {
            value: Js::function("toString", |this, _| {
                if this.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                let join = this.get("join")?;
                if join.is_callable() {
                    join.call(this, vec![])
                } else {
                    Ok(Js::String(this.object_tag_string()))
                }
            }),
            writable: true,
            enumerable: false,
            configurable: true,
        },
    );
    PROTOTYPES.with(|roots| {
        let mut roots = roots.borrow_mut();
        roots.retain(|(r, a)| r.strong_count() > 0 && a.strong_count() > 0);
        roots.push((Rc::downgrade(&realm), Rc::downgrade(&array)));
    });
    Js::Array(array)
}

impl Js {
    /// Native callback entry for the same Array sort body as the installed builtin.
    pub fn sort_array_with(
        &self,
        mut compare: impl FnMut(&Js, &Js) -> Result<std::cmp::Ordering, Thrown>,
    ) -> Result<(), Thrown> {
        let length = self
            .get("length")?
            .to_number()?
            .clamp(0.0, 9_007_199_254_740_991.0) as usize;
        let mut values = Vec::new();
        for index in 0..length {
            if self.has_index(index) {
                values.push(self.get(&index.to_string())?);
            }
        }
        super::super::sorting::stable_sort(&mut values, |x, y| {
            if x.is_undefined() || y.is_undefined() {
                Ok(x.is_undefined().cmp(&y.is_undefined()))
            } else {
                compare(x, y)
            }
        })?;
        let count = values.len();
        for (index, value) in values.into_iter().enumerate() {
            self.set(&index.to_string(), value)?;
        }
        for index in count..length {
            self.delete(&utf16(&index.to_string()))?;
        }
        Ok(())
    }

    pub fn has_index(&self, index: usize) -> bool {
        self.has_property(&utf16(&index.to_string()))
    }
    /// Array.splice mutation phase over live indexed properties, without a raw Vec borrow.
    pub fn splice_array(
        &self,
        start: usize,
        delete_count: usize,
        insert: Vec<Js>,
    ) -> Result<(), Thrown> {
        let Js::Array(array) = self else {
            return Err(Thrown::type_error("splice requires an array"));
        };
        let len = array.len();
        let start = start.min(len);
        let count = delete_count.min(len - start);
        let added = insert.len();
        if added < count {
            for index in start..len - count {
                let from = index + count;
                let to = index + added;
                if self.has_index(from) {
                    self.set(&to.to_string(), self.get(&from.to_string())?)?;
                } else {
                    self.delete(&utf16(&to.to_string()))?;
                }
            }
            for index in (len - count + added..len).rev() {
                self.delete(&utf16(&index.to_string()))?;
            }
        } else if added > count {
            for from in (start + count..len).rev() {
                let to = from as u64 - count as u64 + added as u64;
                if self.has_index(from) {
                    self.set(&to.to_string(), self.get(&from.to_string())?)?;
                } else {
                    self.delete(&utf16(&to.to_string()))?;
                }
            }
        }
        for (index, value) in insert.into_iter().enumerate() {
            self.set(&(start as u64 + index as u64).to_string(), value)?;
        }
        self.set("length", Js::Number((len - count) as f64 + added as f64))
    }
    pub(super) fn own_descriptor(&self, key: &[u16]) -> Option<Prop> {
        if let Js::Array(array) = self
            && key == utf16("length").as_ref()
        {
            return Some(Prop::DataDescriptor {
                value: Js::Number(array.len() as f64),
                writable: array.length_writable.get(),
                enumerable: false,
                configurable: false,
            });
        }
        self.own_props().and_then(|p| p.borrow().get(key).cloned())
    }
    pub(super) fn check_array_growth(&self, key: &[u16]) -> Result<(), Thrown> {
        if let Js::Array(array) = self
            && let Some(index) = array_index(key)
            && index >= array.len()
            && !array.length_writable.get()
        {
            return Err(Thrown::type_error("Cannot assign to read only length"));
        }
        Ok(())
    }
    pub(super) fn grow_array_length(&self, key: &[u16]) {
        if let Js::Array(array) = self
            && let Some(index) = array_index(key)
            && index >= array.len()
        {
            array.length.set(index as u32 + 1);
        }
    }
    pub(super) fn apply_array_length(&self, next: Prop) -> Result<(), Thrown> {
        let Js::Array(array) = self else {
            unreachable!()
        };
        let Prop::DataDescriptor {
            value: Js::Number(length),
            writable,
            ..
        } = next
        else {
            unreachable!("validated length descriptor")
        };
        let old = array.length.get();
        let new = length as u32;
        array.length.set(new);
        if new < old {
            let mut keys: Vec<_> = array
                .props
                .borrow()
                .keys()
                .filter_map(|k| {
                    array_index(k)
                        .filter(|i| *i >= new as usize)
                        .map(|i| (i, k.clone()))
                })
                .collect();
            keys.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
            for (index, key) in keys {
                if !self.reflect_delete(&key)? {
                    array.length.set(index as u32 + 1);
                    array.length_writable.set(writable);
                    return Err(Thrown::type_error(
                        "Cannot delete a non-configurable array index",
                    ));
                }
            }
        }
        array.length_writable.set(writable);
        Ok(())
    }
    pub(in crate::js_runtime) fn symbol_iterator_slot(&self) -> Option<&RefCell<Option<Prop>>> {
        match self {
            Js::Array(a) => Some(&a.symbol_iterator),
            Js::Object(o) => Some(&o.symbol_iterator),
            _ => None,
        }
    }
}

pub(super) fn freeze_prop(property: &mut Prop, frozen: bool) {
    match property {
        Prop::Data(value) => {
            *property = Prop::DataDescriptor {
                value: value.clone(),
                writable: !frozen,
                enumerable: true,
                configurable: false,
            }
        }
        Prop::DataDescriptor {
            writable,
            configurable,
            ..
        } => {
            if frozen {
                *writable = false;
            }
            *configurable = false;
        }
        Prop::Accessor { configurable, .. } => *configurable = false,
    }
}
pub(super) fn frozen_prop(property: &Prop) -> bool {
    match property {
        Prop::Data(_) => false,
        Prop::DataDescriptor {
            writable,
            configurable,
            ..
        } => !writable && !configurable,
        Prop::Accessor { configurable, .. } => !configurable,
    }
}
