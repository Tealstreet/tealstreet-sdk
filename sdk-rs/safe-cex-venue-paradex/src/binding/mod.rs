//! Concrete collaborators for the source workflows, on the shared JS runtime.
mod host;
mod ingress;
mod members;
mod primitives;
mod sdk;
mod sockets;
use safe_cex_exchange::{
    js_runtime::{
        base_exchange::BaseExchangeParts,
        scope::{Scope, ScopeShared},
        value::{Js, Thrown, from_typed},
    },
    runtime::ExchangeError,
};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};
pub(super) type Rt = Scope<State>;
pub(super) type Shared = ScopeShared<State>;
pub(super) struct State {
    pub owner: OnceCell<Js>,
    pub parts: RefCell<Option<BaseExchangeParts>>,
    pub deps: crate::native_exchange::ParadexDependencies,
    pub api_http: RefCell<Option<Rc<dyn safe_cex_exchange::js_runtime::client::JsHttpHost>>>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(Js::Object(o)) = self.owner.get() {
            o.props.borrow_mut().clear();
        }
    }
}
pub(super) fn scope(weak: &Weak<Shared>, this: Js) -> Result<Rt, Thrown> {
    Ok(Rt {
        shared: weak
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Paradex exchange was dropped"))?,
        this,
    })
}
pub(super) fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}
pub(super) fn assemble(
    parts: BaseExchangeParts,
    deps: crate::native_exchange::ParadexDependencies,
) -> Result<Rt, ExchangeError> {
    let args = vec![
        from_typed(&parts.name)?,
        from_typed(&parts.options)?,
        safe_cex_exchange::js_runtime::store::facade(parts.store.clone()),
    ];
    let http = (deps.http)(&parts.options)?;
    let shared = Rc::new_cyclic(|weak| {
        let globals = Js::object(vec![]);
        safe_cex_exchange::js_runtime::globals::install_enums(&globals);
        safe_cex_exchange::js_runtime::globals::install_builtins(&globals, weak.clone());
        safe_cex_exchange::js_runtime::library::install_library(&globals, weak.clone());
        members::install_globals(&globals, weak.clone());
        ScopeShared::new(
            deps.runtime.clone(),
            http,
            parts.store.clone(),
            globals,
            State {
                owner: OnceCell::new(),
                api_http: RefCell::new(None),
                parts: RefCell::new(Some(parts)),
                deps,
            },
        )
    });
    let rt = Rt {
        shared,
        this: Js::Undefined,
    };
    let h = host::Host::new(&rt);
    let owner = crate::runtime_assembly::allocate_exchange(&h, args.clone())?;
    let _ = rt.shared.extra.owner.set(owner.clone());
    *rt.shared.exchange.borrow_mut() = owner.clone();
    let rt = rt.with_this(owner);
    crate::order_execution::initialize_exchange(
        &host::Host::new(&rt),
        args[0].clone(),
        args[1].clone(),
        args[2].clone(),
    )?;
    Ok(rt)
}
