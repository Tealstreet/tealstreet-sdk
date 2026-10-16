//! WOO's scope: the shared js_runtime host handle plus venue-only fields.
use super::value::{Js, Thrown};
use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
use std::{cell::RefCell, rc::Rc};

pub(crate) type Constructor = Rc<dyn Fn(Js, Js, Vec<Js>) -> Result<Js, Thrown>>;
pub(crate) type Initialize = Rc<dyn Fn(Js) -> Result<(), Thrown>>;

/// A class produced by an ops `derive`: its superclass, constructor body, field
/// initializer and prototype members (installed on each receiver it builds).
pub(crate) struct ClassRecord {
    pub class: Js,
    pub superclass: Js,
    pub initialize: Initialize,
    pub prototype: RefCell<Vec<(&'static str, Js)>>,
}

pub(crate) struct WooExtra {
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub classes: RefCell<Vec<Rc<ClassRecord>>>,
}

impl WooExtra {
    pub fn class(&self, class: &Js) -> Option<Rc<ClassRecord>> {
        self.classes
            .borrow()
            .iter()
            .find(|r| r.class.same_reference(class))
            .cloned()
    }
}

pub(crate) type Shared = ScopeShared<WooExtra>;
pub(crate) type Rt = Scope<WooExtra>;

pub(crate) use safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;
