//! Production JavaScript realm for the ported Hyperliquid class bodies.
//!
//! The venue's method bodies are generic over JS host traits; this realm is
//! their production host. Builtins live on intrinsic prototypes, module
//! imports in an explicit table, and native resources behind host objects.
// `to_string`/`to_number`/`from_json` name the ECMAScript abstract operations
// they implement, which take the realm, not a self conversion.
#![allow(clippy::wrong_self_convention)]
pub(crate) mod ccxt;
pub(crate) mod convert;
pub(crate) mod core;
pub(crate) mod json;
pub(crate) mod ops;
pub(crate) mod promise;
pub(crate) mod regex;
mod typed;
pub(crate) mod value;

pub(crate) mod builtins;
pub(crate) mod facade_exchange;
pub(crate) mod facade_rest;
pub(crate) mod facade_socket;
pub(crate) mod facade_store;
mod host_cache;
mod host_classes;
mod host_core;
mod host_income;
mod host_leaves;
mod host_market;
mod host_spot;
mod host_startup;
pub(crate) mod imports;
pub(crate) mod imports_utils;
pub(crate) mod imports_venue;

use safe_cex_exchange::runtime::ExchangeRuntime;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub(crate) use value::{Kind, Obj, ObjectData, PropKey, Thrown, Value};

pub(crate) struct Intrinsics {
    pub object_proto: Obj,
    pub function_proto: Obj,
    pub array_proto: Obj,
    pub string_proto: Obj,
    pub number_proto: Obj,
    pub boolean_proto: Obj,
    pub symbol_proto: Obj,
    pub map_proto: Obj,
    pub set_proto: Obj,
    pub promise_proto: Obj,
    pub iterator_proto: Obj,
    pub error_proto: Obj,
    pub type_error_proto: Obj,
    pub range_error_proto: Obj,
    pub syntax_error_proto: Obj,
    pub regexp_proto: Obj,
    pub date_proto: Obj,
    pub symbol_iterator: Rc<value::Symbol>,
}

pub(crate) struct State {
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub intrinsics: Intrinsics,
    pub globals: Obj,
    pub imports: RefCell<HashMap<String, Value>>,
    /// Module-level values whose identity must be stable across reads.
    pub singletons: RefCell<HashMap<&'static str, Value>>,
    /// The last property read that produced undefined, named in "is not a function".
    pub last_missing: RefCell<Option<String>>,
}

/// Cloning shares the realm; `lexical` is the class binding a method body sees.
#[derive(Clone)]
pub(crate) struct Realm {
    pub state: Rc<State>,
    pub lexical: Option<Value>,
}

fn proto(parent: Option<&Obj>) -> Obj {
    Rc::new(RefCell::new(ObjectData::new(
        Kind::Ordinary,
        parent.cloned(),
    )))
}

impl Realm {
    pub(crate) fn new(runtime: Rc<dyn ExchangeRuntime>) -> Self {
        let object_proto = proto(None);
        let function_proto = proto(Some(&object_proto));
        let error_proto = proto(Some(&object_proto));
        let iterator_proto = proto(Some(&object_proto));
        let intrinsics = Intrinsics {
            function_proto: function_proto.clone(),
            array_proto: proto(Some(&object_proto)),
            string_proto: proto(Some(&object_proto)),
            number_proto: proto(Some(&object_proto)),
            boolean_proto: proto(Some(&object_proto)),
            symbol_proto: proto(Some(&object_proto)),
            map_proto: proto(Some(&object_proto)),
            set_proto: proto(Some(&object_proto)),
            promise_proto: proto(Some(&object_proto)),
            type_error_proto: proto(Some(&error_proto)),
            range_error_proto: proto(Some(&error_proto)),
            syntax_error_proto: proto(Some(&error_proto)),
            regexp_proto: proto(Some(&object_proto)),
            date_proto: proto(Some(&object_proto)),
            iterator_proto,
            error_proto,
            symbol_iterator: Rc::new(value::Symbol {
                description: "Symbol.iterator",
            }),
            object_proto: object_proto.clone(),
        };
        let globals = proto(Some(&object_proto));
        let mut realm = Self {
            state: Rc::new(State {
                runtime,
                intrinsics,
                globals,
                imports: RefCell::default(),
                singletons: RefCell::default(),
                last_missing: RefCell::default(),
            }),
            lexical: None,
        };
        builtins::install(&mut realm);
        realm
    }
    pub(crate) fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.state.runtime.clone()
    }
    pub(crate) fn intrinsics(&self) -> &Intrinsics {
        &self.state.intrinsics
    }
    pub(crate) fn with_lexical(&self, class: &Value) -> Self {
        Self {
            state: self.state.clone(),
            lexical: Some(class.clone()),
        }
    }
    /// Bind a module import the method bodies read by name.
    pub(crate) fn bind_import(&self, name: &str, value: Value) {
        self.state.imports.borrow_mut().insert(name.into(), value);
    }
    pub(crate) fn import(&mut self, name: &str) -> Result<Value, Thrown> {
        let found = self.state.imports.borrow().get(name).cloned();
        match found {
            Some(v) => Ok(v),
            None => self.global_binding(name),
        }
    }
    /// A global identifier read; an unbound name is a ReferenceError.
    pub(crate) fn global_binding(&mut self, name: &str) -> Result<Value, Thrown> {
        if name == "HyperliquidExchange"
            && let Some(class) = &self.lexical
        {
            return Ok(class.clone());
        }
        if let Some(v) = self.state.imports.borrow().get(name) {
            return Ok(v.clone());
        }
        let globals = Value::Object(self.state.globals.clone());
        let key = PropKey::str(name);
        if self.has_property(&globals, &key)? {
            return self.get(&globals, &key);
        }
        Err(self.error_of("ReferenceError", &format!("{name} is not defined")))
    }
    pub(crate) fn set_global(&self, name: &str, value: Value) {
        self.state
            .globals
            .borrow_mut()
            .put(PropKey::str(name), value, false);
    }
    pub(crate) fn singleton(
        &self,
        name: &'static str,
        init: impl FnOnce(&Realm) -> Value,
    ) -> Value {
        if let Some(v) = self.state.singletons.borrow().get(name) {
            return v.clone();
        }
        let v = init(self);
        self.state.singletons.borrow_mut().insert(name, v.clone());
        v
    }
}

#[cfg(test)]
pub(crate) mod test_runtime;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod socket_tests;

#[cfg(test)]
mod spot_limiter_tests;
