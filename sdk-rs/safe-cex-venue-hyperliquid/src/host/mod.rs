//! Hyperliquid host context over the canonical shared JavaScript runtime.
use safe_cex_exchange::{
    js_runtime::{
        client::{JsHttpHost, rest_http_host},
        globals::install_builtins,
        library::install_library,
        scope::ScopeShared,
        value::{FunctionData, Js, Thrown},
    },
    runtime::ExchangeRuntime,
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_transport::RestAdapter;
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}, sync::Arc};

pub(crate) struct Modules {
    imports: RefCell<HashMap<String, Js>>,
    singletons: RefCell<HashMap<&'static str, Js>>,
}

#[derive(Clone)]
pub(crate) struct Host {
    pub shared: Rc<ScopeShared<Modules>>,
    lexical: Option<Weak<FunctionData>>,
}

impl Host {
    pub(crate) fn new(runtime: Rc<dyn WsRuntime>, http: Arc<dyn RestAdapter>, store: Rc<dyn Store>) -> Self {
        let globals = Js::object(vec![]);
        let http: Rc<dyn JsHttpHost> = rest_http_host(runtime.clone(), http);
        let shared = Rc::new(ScopeShared::new(runtime, http, store, globals.clone(), Modules {
            imports: RefCell::default(), singletons: RefCell::default(),
        }));
        install_builtins(&globals, Rc::downgrade(&shared));
        install_library(&globals, Rc::downgrade(&shared));
        Self { shared, lexical: None }
    }
    pub(crate) fn runtime(&self) -> Rc<dyn ExchangeRuntime> { self.shared.runtime.clone() }
    pub(crate) fn with_lexical(&self, class: &Js) -> Self {
        let Js::Function(class) = class else { panic!("class binding must be a function") };
        Self { shared: self.shared.clone(), lexical: Some(Rc::downgrade(class)) }
    }
    pub(crate) fn bind_import(&self, name: &str, value: Js) {
        self.shared.extra.imports.borrow_mut().insert(name.into(), value);
    }
    pub(crate) fn import(&mut self, name: &str) -> Result<Js, Thrown> { self.global_binding(name) }
    pub(crate) fn global_binding(&mut self, name: &str) -> Result<Js, Thrown> {
        if name == "HyperliquidExchange" && let Some(class) = self.lexical.as_ref().and_then(Weak::upgrade) {
            return Ok(Js::Function(class));
        }
        if let Some(value) = self.shared.extra.imports.borrow().get(name) { return Ok(value.clone()); }
        if self.shared.globals.has_own(name) { return self.shared.globals.get(name); }
        Err(Thrown::error("ReferenceError", &format!("{name} is not defined")))
    }
    pub(crate) fn set_global(&self, name: &str, value: Js) {
        self.shared.globals.define(name, value).expect("fresh host global is extensible");
    }
    pub(crate) fn singleton(&self, name: &'static str, init: impl FnOnce(&Host) -> Js) -> Js {
        if let Some(value) = self.shared.extra.singletons.borrow().get(name) { return value.clone(); }
        let value = init(self);
        self.shared.extra.singletons.borrow_mut().insert(name, value.clone());
        value
    }
    pub(crate) fn has_binding(&self, name: &str) -> bool {
        self.shared.extra.imports.borrow().contains_key(name) || self.shared.globals.has_own(name)
    }
}

pub(crate) mod operations;
pub(crate) mod host_classes;
pub(crate) mod host_core;
pub(crate) mod host_income;
pub(crate) mod host_leaves;
pub(crate) mod host_market;
pub(crate) mod host_spot;
pub(crate) mod host_startup;
pub(crate) mod host_cache;
pub(crate) mod imports;
pub(crate) mod imports_utils;
pub(crate) mod imports_venue;
pub(crate) mod facade_exchange;
pub(crate) mod facade_rest;
pub(crate) mod facade_socket;
pub(crate) mod facade_store;
pub(crate) mod ops;

#[cfg(test)]
pub(crate) mod test_runtime;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod socket_tests;
#[cfg(test)]
mod spot_limiter_tests;
#[cfg(test)]
mod table_hook_tests;

mod operators;
