//! Crypto.com's scope: the shared js_runtime host handle plus venue-only fields.
use safe_cex_exchange::js_runtime::{
    scope::{Scope, ScopeShared},
    value::{Js, Thrown},
};
use std::{cell::RefCell, rc::Rc};

pub(crate) type ClassBody = Rc<dyn Fn(Js, Js, Vec<Js>) -> Result<Js, Thrown>>;
pub(crate) type Getter = Rc<dyn Fn(Js) -> Result<Js, Thrown>>;

/// A class the bodies derived: its constructor body and the prototype members
/// installed on it.
pub(crate) struct Class {
    pub class: Js,
    pub base: Js,
    pub body: ClassBody,
    pub prototype: Js,
    pub getters: RefCell<Vec<(String, Getter)>>,
}

pub(crate) struct CryptocomExtra {
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub classes: RefCell<Vec<Rc<Class>>>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

pub use safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;

pub(crate) type Shared = ScopeShared<CryptocomExtra>;
pub(crate) type Rt = Scope<CryptocomExtra>;

pub(crate) fn class_of(rt: &Rt, class: &Js) -> Option<Rc<Class>> {
    rt.shared
        .extra
        .classes
        .borrow()
        .iter()
        .find(|c| c.class.same_reference(class))
        .cloned()
}
