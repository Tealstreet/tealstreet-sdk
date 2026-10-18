use super::{Js, JsFuture, Thrown, await_js};
use safe_cex_exchange::js_runtime::{
    base_exchange::ExternalPublicDataFactory,
    scope::{Scope, ScopeShared},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{cell::RefCell, rc::Rc};

pub(super) struct Extra {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub local_to_utc: Rc<dyn Fn(f64) -> f64>,
    pub prototypes: RefCell<Vec<(crate::classes::ClassKind, Js)>>,
}
pub(super) type Shared = ScopeShared<Extra>;
#[derive(Clone)]
pub(super) struct Host(pub Scope<Extra>);
impl Host {
    pub fn weak(&self) -> Rc<dyn Fn() -> Result<Self, Thrown>> {
        let shared = Rc::downgrade(&self.0.shared);
        let receiver = match &self.0.this {
            Js::Object(value) => Some(Rc::downgrade(value)),
            _ => None,
        };
        Rc::new(move || {
            let shared = shared
                .upgrade()
                .ok_or_else(|| Thrown::type_error("WEEX owner was released"))?;
            let this = match &receiver {
                Some(value) => Js::Object(
                    value
                        .upgrade()
                        .ok_or_else(|| Thrown::type_error("WEEX receiver was released"))?,
                ),
                None => Js::Undefined,
            };
            Ok(Self(Scope { shared, this }))
        })
    }
    pub fn this(&self) -> Js {
        self.0.this.clone()
    }
    pub fn root(&self, path: &str) -> Result<Js, Thrown> {
        self.0.path(path)
    }
    pub fn global(&self, path: &str) -> Result<Js, Thrown> {
        self.0.global(path)
    }
    pub fn call(&self, path: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.0.call_path(path, args)
    }
    pub fn spawn(&self, body: JsFuture<'static>) -> Js {
        self.0.spawn(body)
    }
    pub fn request(
        &self,
        method: &str,
        endpoint: Js,
        config: Js,
    ) -> Result<JsFuture<'static>, Thrown> {
        let xhr = self.root("xhr")?;
        let pending = xhr.call_method(method, std::vec![endpoint, config])?;
        Ok(Box::pin(await_js(pending)))
    }
    pub fn warn(&self, message: Js) -> Result<(), Thrown> {
        self.call("log", std::vec![message, Js::str("warning")])?;
        Ok(())
    }
    pub fn emit(&self, message: Js) -> Result<(), Thrown> {
        self.call("emitAndLogError", std::vec![message])?;
        Ok(())
    }
}
