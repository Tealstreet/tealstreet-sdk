//! Name -> Rust constructor table for bound venues, selected per name by the caller.
//! Design and membership rules: ../CONSTRUCTION.md.
use crate::module_host::{
    ExchangeConstructor, ModuleError, NativeConstructorResult, NativeModuleHost,
};
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::ExchangeError,
    websocket::WsRuntime,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_backpack::{
    adapter::{BackpackExchange, DateParser},
    native_http::{BuildHttp, InstallTransport},
};
use safe_cex_venue_binance::exchange_classes::{BinanceExchange, ExchangeDependencies};
use std::rc::Rc;

/// What `BackpackExchange::new` takes besides the BaseExchange the registry builds.
pub struct BackpackDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub build_http: BuildHttp,
    pub install_transport: InstallTransport,
    pub date_parse: DateParser,
}

/// Host inputs for Rust venue construction, requested once per constructed instance.
pub trait RustVenueHost {
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError>;
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError>;
}

/// The P4 flag: `ModuleHost` (the default) keeps today's TypeScript-module route.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConstructionPath {
    #[default]
    ModuleHost,
    Rust,
}

type RustConstructorEntry = fn(Rc<dyn RustVenueHost>) -> ExchangeConstructor;

/// Bound venues only; each later venue is added by its own follow-up.
pub const RUST_VENUES: &[(ExchangeName, RustConstructorEntry)] = &[
    (ExchangeName::Backpack, backpack_constructor),
    (ExchangeName::Binance, binance_constructor),
];

pub fn backpack_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.backpack()?;
        let base = BaseExchange::new(
            name,
            options,
            store,
            deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        let exchange = BackpackExchange::new(
            base,
            deps.build_http,
            deps.install_transport,
            deps.runtime,
            deps.date_parse,
        )?;
        Ok(exchange)
    })
}

pub fn binance_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = BinanceExchange::new(name, options, store, host.binance()?)
            .map_err(Rc::unwrap_or_clone)?;
        Ok(Rc::new(exchange))
    })
}

pub fn rust_constructor(
    name: &ExchangeName,
    host: Rc<dyn RustVenueHost>,
) -> Option<ExchangeConstructor> {
    RUST_VENUES
        .iter()
        .find(|(venue, _)| venue == name)
        .map(|(_, entry)| entry(host))
}

fn wire_name(name: &ExchangeName) -> Result<String, std::fmt::Error> {
    let value = serde_json::to_value(name).map_err(|_| std::fmt::Error)?;
    value.as_str().map(str::to_owned).ok_or(std::fmt::Error)
}

/// `ConstructionPath::Rust` was chosen for a name that has no table entry.
#[derive(Debug)]
pub struct NoRustConstructor(pub ExchangeName);
impl std::fmt::Display for NoRustConstructor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no Rust constructor for {}", wire_name(&self.0)?)
    }
}
impl std::error::Error for NoRustConstructor {}

/// The module-host registry has no loader entry for this name.
#[derive(Debug)]
pub struct NoModuleLoader(pub ExchangeName);
impl std::fmt::Display for NoModuleLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no module loader for {}", wire_name(&self.0)?)
    }
}
impl std::error::Error for NoModuleLoader {}

/// `Rust` for an unbound name is an error: a flag that lands on TypeScript is unfalsifiable.
pub fn native_constructor(
    name: ExchangeName,
    path: ConstructionPath,
    modules: &dyn NativeModuleHost,
    rust: Rc<dyn RustVenueHost>,
) -> NativeConstructorResult {
    match path {
        ConstructionPath::ModuleHost => {
            let registration = crate::loaders::EXCHANGE_LOADERS
                .iter()
                .find(|registration| registration.name == name)
                .ok_or_else(|| -> ModuleError { Rc::new(NoModuleLoader(name.clone())) })?;
            (registration.native)(modules)
        }
        ConstructionPath::Rust => rust_constructor(&name, rust)
            .map(|constructor| std::future::ready(Some(constructor)))
            .ok_or_else(|| -> ModuleError { Rc::new(NoRustConstructor(name)) }),
    }
}
