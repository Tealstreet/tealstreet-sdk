use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, await_js},
    scope::{Scope, ScopeShared},
    value::{Js, Thrown},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};

pub use safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;

pub type TransportInstaller = std::rc::Rc<dyn Fn(Js, Js, ExchangeName) -> Result<(), Thrown>>;

pub(crate) struct Extra {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub install_transport: Option<TransportInstaller>,
}
pub(crate) type Shared = ScopeShared<Extra>;
pub(crate) type Rt = Scope<Extra>;

#[derive(Clone)]
pub(crate) struct Host {
    pub rt: Rt,
}

impl Host {
    pub(crate) fn get(&self, name: &str) -> Result<Js, Thrown> {
        self.rt.this.get(name)
    }
    pub(crate) fn store(&self, name: &str) -> Result<Js, Thrown> {
        self.get("store")?.get(name)
    }
    pub(crate) fn call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.rt.this.call_method(name, args)
    }
    pub(crate) fn invoke(&self, name: &str, args: Vec<Js>) -> JsFuture<'static> {
        let result = self.call(name, args);
        Box::pin(async move { await_js(result?).await })
    }
    pub(crate) fn global(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.rt.global(name)?.call(Js::Undefined, args)
    }
    pub(crate) fn http(
        &self,
        client: &str,
        verb: &str,
        endpoint: &str,
        args: Vec<Js>,
    ) -> JsFuture<'static> {
        let result = (|| {
            let mut a = vec![Js::str(endpoint)];
            a.extend(args);
            self.get(client)?.call_method(verb, a)
        })();
        Box::pin(async move { await_js(result?).await })
    }
    pub(crate) fn emit(&self, message: Js) -> Result<(), Thrown> {
        self.call("emitAndLogError", vec![message])?;
        Ok(())
    }
    pub(crate) fn report(&self, error: &Thrown) -> Result<(), Thrown> {
        self.call("emitAndLogError", vec![error.value.clone()])?;
        Ok(())
    }
    pub(crate) fn child(&self, this: Js) -> Self {
        Self {
            rt: self.rt.with_this(this),
        }
    }
}

pub(crate) fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
pub(crate) fn rows(value: &Js) -> Result<Vec<Js>, Thrown> {
    value
        .array_values()
        .ok_or_else(|| Thrown::type_error("value is not iterable"))
}
pub(crate) fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}
