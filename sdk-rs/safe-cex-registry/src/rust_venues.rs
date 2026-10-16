//! Name -> Rust constructor table for bound venues, selected per name by the caller.
//! Design and membership rules: ../CONSTRUCTION.md.
use crate::module_host::{
    ExchangeConstructor, ModuleError, NativeConstructorResult, NativeModuleHost,
};
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    external::ExternalPublicData,
    http::HttpClient,
    js_runtime::base_exchange::{ExternalPublicDataFactory, HostedSigner},
    runtime::ExchangeError,
    websocket::WsRuntime,
};
use safe_cex_transport::RestAdapter;
use safe_cex_types::{ExchangeName, ExchangeOptions, ExchangeOptionsExternalPublicData};
use safe_cex_venue_backpack::{
    adapter::{BackpackExchange, DateParser},
    native_http::{BuildHttp, InstallTransport},
};
use safe_cex_venue_binance::exchange_classes::{
    BinanceExchange, BinancePMExchange, BinanceSpotExchange, ExchangeDependencies,
};
use safe_cex_venue_bitget::{BitgetDependencies, BitgetExchange};
use safe_cex_venue_bitunix::adapter::BitunixVenue;
use safe_cex_venue_blofin::native_exchange::BlofinNativeExchange;
use safe_cex_venue_kraken::{KrakenDependencies, KrakenExchange};
use safe_cex_venue_okx::binding::exchange::{OkxDependencies, OkxExchange};
use std::rc::Rc;

/// What `BackpackExchange::new` takes besides the BaseExchange the registry builds.
pub struct BackpackDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub build_http: BuildHttp,
    pub install_transport: InstallTransport,
    pub date_parse: DateParser,
}

/// Bitunix's createAPI client builder and transport install, per lane.
pub type BitunixBuild = Rc<dyn Fn(&ExchangeOptions) -> Result<HttpClient, ExchangeError>>;
pub type BitunixInstall = Rc<dyn Fn(&HttpClient) -> Result<(), ExchangeError>>;

/// What `BitunixVenue::new` takes besides the BaseExchange the registry builds.
pub struct BitunixDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub build: BitunixBuild,
    pub install: BitunixInstall,
    /// Required when `options.externalPublicData` is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub remote_signer_account_name: Option<String>,
}

/// The common HTTP stack BloFin builds each client from: (options, baseURL, timeout budget).
pub type BlofinHttp =
    Rc<dyn Fn(&ExchangeOptions, &str, i64) -> Result<Rc<dyn RestAdapter>, String>>;

/// What `BlofinNativeExchange::new` takes besides the BaseExchange the registry builds.
pub struct BlofinDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: BlofinHttp,
    /// Required when `options.externalPublicData` is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub remote_signer_account_name: Option<String>,
}

/// Host inputs for Rust venue construction, requested once per constructed instance.
pub trait RustVenueHost {
    /// AsterDEX needs a per-lane HTTP factory to preserve secondary routing.
    fn asterdex(
        &self,
    ) -> Result<safe_cex_venue_asterdex::native_exchange::AsterDEXDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "asterdex inputs are unavailable on this host",
        ))
    }

    /// Optional hosted account capability; no host-local secret is required.
    fn hosted_signer(&self) -> Option<HostedSigner> {
        None
    }
    fn bingx(&self) -> Result<safe_cex_venue_bingx::BingxDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "BingX dependencies are unavailable on this host",
        ))
    }
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError>;
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError>;
    /// OKX, OKX Demo, OKX EU and OKX EU Demo share one input set.
    fn okx(&self) -> Result<OkxDependencies, ExchangeError>;
    fn blofin(&self) -> Result<BlofinDependencies, ExchangeError>;
    fn bitunix(&self) -> Result<BitunixDependencies, ExchangeError>;
    /// Futures and Spot share the SDK runtime's nonce and client-ID state.
    fn kraken(&self) -> Result<KrakenDependencies, ExchangeError>;
    /// Bitget, Bitget USDC and Bitget inverse share one input set.
    fn bitget(&self) -> Result<BitgetDependencies, ExchangeError>;
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
    (ExchangeName::Asterdex, asterdex_constructor),
    (ExchangeName::Backpack, backpack_constructor),
    (ExchangeName::Binance, binance_constructor),
    (ExchangeName::Binancepm, binancepm_constructor),
    (ExchangeName::Binancespot, binancespot_constructor),
    (ExchangeName::Bingx, bingx_constructor),
    (ExchangeName::Bitget, bitget_constructor),
    (ExchangeName::Bitgetinverse, bitget_constructor),
    (ExchangeName::Bitgetusdc, bitget_constructor),
    (ExchangeName::Bitunix, bitunix_constructor),
    (ExchangeName::Blofin, blofin_constructor),
    (ExchangeName::Kraken, kraken_constructor),
    (ExchangeName::Krakenspot, kraken_constructor),
    (ExchangeName::Okx, okx_constructor),
    (ExchangeName::Okxdemo, okx_constructor),
    (ExchangeName::Okxeu, okx_constructor),
    (ExchangeName::Okxeudemo, okx_constructor),
];

pub fn asterdex_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(
            safe_cex_venue_asterdex::native_exchange::AsterDEXExchange::new(
                name,
                options,
                store,
                host.asterdex()?,
            )?,
        )
    })
}

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

pub fn binancepm_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = BinancePMExchange::new(name, options, store, host.binance()?)
            .map_err(Rc::unwrap_or_clone)?;
        Ok(Rc::new(exchange))
    })
}

pub fn binancespot_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = BinanceSpotExchange::new(name, options, store, host.binance()?)
            .map_err(Rc::unwrap_or_clone)?;
        Ok(Rc::new(exchange))
    })
}

/// The provider factory, or a refusal when `externalPublicData` is set without one.
fn external_factory(
    venue: &str,
    factory: Option<ExternalPublicDataFactory>,
    options: &ExchangeOptions,
) -> Result<ExternalPublicDataFactory, ExchangeError> {
    match factory {
        Some(factory) => Ok(factory),
        None if options.external_public_data.is_some() => Err(ExchangeError::new(format!(
            "{venue}: externalPublicData is configured but no provider was supplied"
        ))),
        None => Ok(Rc::new(
            |_: &ExchangeOptionsExternalPublicData,
             _: Rc<BaseExchange>|
             -> Rc<dyn ExternalPublicData> {
                unreachable!("externalPublicData is unset, so no provider is built")
            },
        )),
    }
}

pub fn bitget_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = BitgetExchange::new(name, options, store, host.bitget()?)?;
        Ok(exchange)
    })
}

pub fn bitunix_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.bitunix()?;
        let external = external_factory("bitunix", deps.external_public_data, &options)?;
        let mut base = BaseExchange::new(
            name,
            options,
            store,
            deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);
        let exchange = BitunixVenue::new(
            base,
            deps.build,
            deps.install,
            deps.runtime,
            external,
            deps.remote_signer_account_name,
        )?;
        Ok(exchange)
    })
}

pub fn blofin_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.blofin()?;
        let external = external_factory("blofin", deps.external_public_data, &options)?;
        let mut base = BaseExchange::new(
            name,
            options,
            store,
            deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);
        let http = deps.http;
        let exchange = BlofinNativeExchange::new(
            base,
            move |options, base_url, budget| http(options, base_url, budget),
            deps.runtime,
            external,
            deps.remote_signer_account_name,
        )?;
        Ok(exchange)
    })
}

pub fn kraken_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = KrakenExchange::new(name, options, store, host.kraken()?)?;
        Ok(exchange)
    })
}

pub fn okx_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = OkxExchange::new(name, options, store, host.okx()?)?;
        Ok(exchange)
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

pub fn bingx_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        safe_cex_venue_bingx::BingxExchange::new(name, options, store, host.bingx()?)
            .map(|venue| venue as Rc<dyn safe_cex_exchange::exchange::Exchange>)
    })
}
