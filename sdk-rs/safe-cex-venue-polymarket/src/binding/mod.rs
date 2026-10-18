//! Concrete adapters for the source bodies, using the shared ECMAScript runtime.
pub mod exchange;
mod globals;
mod host;
mod members;
mod signing;
mod socket;
use safe_cex_exchange::js_runtime::{
    scope::{Scope, ScopeShared},
    value::{Js, Thrown},
};
use std::rc::Rc;

pub(crate) struct Extra {
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub external: Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>,
    pub classes: std::cell::OnceCell<Rc<crate::classes::PolymarketClasses<Rt>>>,
}
pub(crate) type Rt = Scope<Extra>;
pub(crate) type Shared = ScopeShared<Extra>;

pub(crate) fn error(message: &str) -> Thrown {
    Thrown::error("Error", message)
}
pub(crate) fn text(value: &Js) -> Result<String, Thrown> {
    String::from_utf16(&value.to_js_string()?)
        .map_err(|_| error("Unsupported lone UTF-16 surrogate"))
}
pub(crate) struct Iter {
    iterator: Js,
    next: Js,
}
impl Iter {
    fn new(value: Js) -> Result<Self, Thrown> {
        let iterator = value.iterator()?;
        let next = iterator.get("next")?;
        Ok(Self { iterator, next })
    }
    fn next(&mut self) -> Result<Option<Js>, Thrown> {
        let step = self.next.call(self.iterator.clone(), vec![])?;
        if step.get("done")?.truthy() {
            Ok(None)
        } else {
            step.get("value").map(Some)
        }
    }
}

pub(crate) fn retained_task(rt: &Rt, task: impl std::future::Future<Output = ()> + 'static) {
    // Promise owns its continuation, including when the original observer drops.
    let _ = safe_cex_exchange::js_runtime::promise::Promise::spawn(
        &rt.exchange_runtime(),
        Box::pin(async move {
            task.await;
            Ok(Js::Undefined)
        }),
    );
}

pub(crate) fn async_fn(
    rt: &Rt,
    name: &'static str,
    body: Rc<dyn Fn(Vec<Js>) -> crate::exchange::PinTask<Js, Thrown>>,
) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function(name, move |_, args| {
        let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
        Ok(rt.spawn(body(args)))
    })
}
