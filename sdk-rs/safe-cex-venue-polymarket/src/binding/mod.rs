//! Concrete adapters for the source bodies, using the shared ECMAScript runtime.
pub mod exchange;
mod globals;
mod host;
mod members;
mod signing;
mod socket;
mod throttle;
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
    value: Js,
    index: usize,
}
impl Iter {
    fn new(value: Js) -> Result<Self, Thrown> {
        let value = match value {
            Js::Array(_) => value,
            Js::Set(ref set) => Js::array(set.borrow().clone()),
            Js::Map(ref map) => Js::array(
                map.borrow()
                    .iter()
                    .map(|(k, v)| Js::array(vec![k.clone(), v.clone()]))
                    .collect(),
            ),
            Js::String(ref s) => Js::array(
                String::from_utf16_lossy(s)
                    .chars()
                    .map(|c| Js::str(&c.to_string()))
                    .collect(),
            ),
            _ => return Err(Thrown::type_error("value is not iterable")),
        };
        Ok(Self { value, index: 0 })
    }
    fn next(&mut self) -> Result<Option<Js>, Thrown> {
        let length = self.value.get("length")?.to_number()? as usize;
        if self.index >= length {
            return Ok(None);
        }
        let value = self.value.get(&self.index.to_string())?;
        self.index += 1;
        Ok(Some(value))
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
