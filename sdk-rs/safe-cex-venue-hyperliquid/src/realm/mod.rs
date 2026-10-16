//! Production JavaScript realm for the ported Hyperliquid class bodies.
//!
//! The venue's method bodies are generic over JS host traits; this realm is
//! their production host. Builtins live on intrinsic prototypes, module
//! imports in an explicit table, and native resources behind host objects.
pub mod convert;
pub mod core;
pub mod json;
pub mod ops;
pub mod promise;
pub mod regex;
pub mod value;

pub(crate) mod builtins;
pub mod facade_exchange;
pub mod facade_rest;
pub mod facade_socket;
pub mod facade_store;
pub mod imports;
mod host_classes;
mod host_core;
mod host_leaves;

use crate::spot_json::JsString;
use safe_cex_exchange::runtime::ExchangeRuntime;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub use value::{Kind, NativeFn, Obj, ObjectData, PropKey, Thrown, Value};

pub struct Intrinsics {
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

pub struct State {
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub intrinsics: Intrinsics,
    pub globals: Obj,
    pub imports: RefCell<HashMap<String, Value>>,
    /// Unhandled rejections already reported, so each is reported once.
    pub reported: RefCell<Vec<Obj>>,
    /// Module-level values whose identity must be stable across reads.
    pub singletons: RefCell<HashMap<&'static str, Value>>,
}

/// Cloning shares the realm; `lexical` is the class binding a method body sees.
#[derive(Clone)]
pub struct Realm {
    pub state: Rc<State>,
    pub lexical: Option<Value>,
}

fn proto(parent: Option<&Obj>) -> Obj {
    Rc::new(RefCell::new(ObjectData::new(Kind::Ordinary, parent.cloned())))
}

impl Realm {
    pub fn new(runtime: Rc<dyn ExchangeRuntime>) -> Self {
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
                reported: RefCell::default(),
                singletons: RefCell::default(),
            }),
            lexical: None,
        };
        builtins::install(&mut realm);
        realm
    }
    pub fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.state.runtime.clone()
    }
    pub fn intrinsics(&self) -> &Intrinsics {
        &self.state.intrinsics
    }
    pub fn with_lexical(&self, class: &Value) -> Self {
        Self {
            state: self.state.clone(),
            lexical: Some(class.clone()),
        }
    }
    /// Bind a module import the method bodies read by name.
    pub fn bind_import(&self, name: &str, value: Value) {
        self.state.imports.borrow_mut().insert(name.into(), value);
    }
    pub fn import(&mut self, name: &str) -> Result<Value, Thrown> {
        let found = self.state.imports.borrow().get(name).cloned();
        match found {
            Some(v) => Ok(v),
            None => self.global_binding(name),
        }
    }
    /// A global identifier read; an unbound name is a ReferenceError.
    pub fn global_binding(&mut self, name: &str) -> Result<Value, Thrown> {
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
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.state
            .globals
            .borrow_mut()
            .put(PropKey::str(name), value, false);
    }
    pub fn singleton(&self, name: &'static str, init: impl FnOnce(&Realm) -> Value) -> Value {
        if let Some(v) = self.state.singletons.borrow().get(name) {
            return v.clone();
        }
        let v = init(self);
        self.state.singletons.borrow_mut().insert(name, v.clone());
        v
    }
    pub fn js_string(&self, value: &Value) -> Option<JsString> {
        value.as_units().map(|u| JsString(u.to_vec()))
    }
}

#[cfg(test)]
pub(crate) mod test_runtime;
#[cfg(test)]
mod tests;
