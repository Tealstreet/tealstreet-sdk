//! Concrete primitive bindings for the existing Orderly source bodies.
mod host;
mod members;
mod primitives;
mod sockets;
use safe_cex_exchange::{
    js_runtime::{
        base_exchange::BaseExchangeParts,
        scope::{Scope, ScopeShared},
        value::{Js, Thrown},
    },
    runtime::ExchangeError,
};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};
pub(crate) type Shared = ScopeShared<State>;
pub(crate) type Rt = Scope<State>;
pub(crate) struct State {
    parts: RefCell<Option<BaseExchangeParts>>,
    deps: crate::native_exchange::OrderlyDependencies,
    owner: OnceCell<Js>,
    prototype: Js,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(Js::Object(owner)) = self.owner.get() {
            owner.props.borrow_mut().clear();
        }
        if let Js::Object(prototype) = &self.prototype {
            prototype.props.borrow_mut().clear();
        }
    }
}
fn scope(weak: &Weak<Shared>, this: Js) -> Result<Rt, Thrown> {
    Ok(Rt {
        shared: weak
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Orderly exchange was dropped"))?,
        this,
    })
}
fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}
pub(crate) fn assemble(
    parts: BaseExchangeParts,
    deps: crate::native_exchange::OrderlyDependencies,
    kind: crate::classes::Kind,
) -> Result<Rt, ExchangeError> {
    let name = safe_cex_exchange::js_runtime::value::from_typed(&parts.name)?;
    let options = safe_cex_exchange::js_runtime::value::from_typed(&parts.options)?;
    let store = safe_cex_exchange::js_runtime::store::facade(parts.store.clone())?;
    let http = (deps.http)(&parts.options)?;
    let shared = Rc::new_cyclic(|weak| {
        let globals = Js::object(vec![]);
        safe_cex_exchange::js_runtime::globals::install_builtins(&globals, weak.clone());
        safe_cex_exchange::js_runtime::globals::install_enums(&globals);
        safe_cex_exchange::js_runtime::library::install_library(&globals, weak.clone());
        ScopeShared::new(
            deps.runtime.clone(),
            http,
            parts.store.clone(),
            globals,
            State {
                parts: RefCell::new(Some(parts)),
                deps,
                owner: OnceCell::new(),
                prototype: Js::object(vec![]),
            },
        )
    });
    let rt = Rt {
        shared,
        this: Js::Undefined,
    };
    let this = crate::classes::construct(
        &host::Host::new(&rt),
        kind,
        vec![Rc::new(name), Rc::new(options), Rc::new(store)],
    )?;
    let this = (*this).clone();
    let _ = rt.shared.extra.owner.set(this.clone());
    *rt.shared.exchange.borrow_mut() = this.clone();
    Ok(rt.with_this(this))
}
