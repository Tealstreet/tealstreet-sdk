//! Explicit host boundary for module loading and native constructor values.
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    error::Error,
    future::{Future, Ready},
    pin::Pin,
    rc::Rc,
};

/// Original host failure identity is retained across load and export lookup.
pub type ModuleError = Rc<dyn Error>;
/// Browser-local module hosts and constructors deliberately do not require Send.
pub type ModuleFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Rc<dyn ExchangeModule>, ModuleError>> + 'a>>;
pub type ConstructorFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<ExchangeConstructor>, ModuleError>> + 'a>>;
pub type NativeConstructorResult = Result<Ready<Option<ExchangeConstructor>>, ModuleError>;

type ConstructorFactory =
    dyn Fn(ExchangeName, ExchangeOptions, Rc<dyn Store>) -> Result<Rc<dyn Exchange>, ExchangeError>;

/// A callable constructor capability, not a module/constructor-name descriptor.
/// Hosts capture their runtime, transport and scheduling dependencies in the factory.
#[derive(Clone)]
pub struct ExchangeConstructor {
    factory: Rc<ConstructorFactory>,
}
impl ExchangeConstructor {
    pub fn new(
        factory: impl Fn(
            ExchangeName,
            ExchangeOptions,
            Rc<dyn Store>,
        ) -> Result<Rc<dyn Exchange>, ExchangeError>
        + 'static,
    ) -> Self {
        Self {
            factory: Rc::new(factory),
        }
    }
    pub fn construct(
        &self,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<Rc<dyn Exchange>, ExchangeError> {
        (self.factory)(name, options, store)
    }
    pub fn same_constructor(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.factory, &other.factory)
    }
}

/// Lookup stays on the host namespace, allowing live bindings and getter failures.
/// Missing export is source undefined; the registry never constructs a fallback.
pub trait ExchangeModule {
    fn constructor_export(
        &self,
        export: &'static str,
    ) -> Result<Option<ExchangeConstructor>, ModuleError>;
}
/// import() starts when the registry loader is invoked, before future polling.
pub trait WebModuleHost {
    fn import_module(&self, specifier: &'static str) -> ModuleFuture<'_>;
}
/// require() and subsequent export lookup finish synchronously before Ready exists.
pub trait NativeModuleHost {
    fn require_module(
        &self,
        specifier: &'static str,
    ) -> Result<Rc<dyn ExchangeModule>, ModuleError>;
}

pub(crate) fn web_load<'a>(
    host: &'a dyn WebModuleHost,
    specifier: &'static str,
    export: &'static str,
) -> ConstructorFuture<'a> {
    let imported = host.import_module(specifier);
    Box::pin(async move { imported.await?.constructor_export(export) })
}
pub(crate) fn native_load(
    host: &dyn NativeModuleHost,
    specifier: &'static str,
    export: &'static str,
) -> NativeConstructorResult {
    let module = host.require_module(specifier)?;
    let constructor = module.constructor_export(export)?;
    Ok(std::future::ready(constructor))
}
