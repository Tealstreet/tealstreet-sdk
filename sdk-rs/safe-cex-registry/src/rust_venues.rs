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
use safe_cex_venue_coinbase::{CoinbaseDependencies, CoinbaseExchange};
use safe_cex_venue_grvt::binding::exchange::{GrvtDependencies, GrvtExchange};
use safe_cex_venue_kraken::{KrakenDependencies, KrakenExchange};
use safe_cex_venue_lighter::binding::exchange::{LighterDependencies, LighterNativeExchange};
use safe_cex_venue_okx::binding::exchange::{OkxDependencies, OkxExchange};
use safe_cex_venue_polymarket::binding::exchange::{PolymarketDependencies, PolymarketExchange};
use safe_cex_venue_weex::{WeexDependencies, WeexExchange};
use safe_cex_venue_woo::{WooDependencies, WooExchange};
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

/// Native collaborators of the already-bound MEXC exchange.
pub struct MexcDependencies {
    pub current_time_ms: Rc<dyn Fn() -> f64>,
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn RestAdapter>,
    pub local_to_utc: Rc<dyn Fn(f64) -> f64>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

/// Host inputs for Rust venue construction, requested once per constructed instance.
pub trait RustVenueHost {
    /// Per-name inputs for the six source Orderly subclasses.
    fn orderly(
        &self,
        _: ExchangeName,
    ) -> Result<safe_cex_venue_orderly::native_exchange::OrderlyDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Orderly dependencies were not supplied by this host",
        ))
    }
    fn kucoin_external_public_data(&self) -> Option<ExternalPublicDataFactory> {
        None
    }
    fn kucoin(
        &self,
        _: ExchangeName,
        _: &ExchangeOptions,
    ) -> Result<safe_cex_venue_kucoin::kucoin_exchange::KucoinDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "KuCoin dependencies were not supplied by this host",
        ))
    }
    fn mexc(&self, _: &ExchangeOptions) -> Result<MexcDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "MEXC dependencies were not supplied by this host",
        ))
    }
    /// AsterDEX needs a per-lane HTTP factory to preserve secondary routing.
    fn asterdex(
        &self,
    ) -> Result<safe_cex_venue_asterdex::native_exchange::AsterDEXDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "asterdex inputs are unavailable on this host",
        ))
    }

    fn cryptocom(
        &self,
    ) -> Result<safe_cex_venue_cryptocom::binding::exchange::CryptocomDependencies, ExchangeError>
    {
        Err(ExchangeError::new(
            "cryptocom inputs are unavailable on this host",
        ))
    }

    fn paradex(
        &self,
    ) -> Result<safe_cex_venue_paradex::native_exchange::ParadexDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Paradex dependencies were not supplied by this host",
        ))
    }

    fn phemex(
        &self,
    ) -> Result<safe_cex_venue_phemex::binding::exchange::PhemexDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "phemex inputs are unavailable on this host",
        ))
    }

    /// Native inputs for the source's Phase-0 scaffold; start still refuses support.
    fn txflow(
        &self,
        _: &ExchangeOptions,
    ) -> Result<safe_cex_exchange::http::HttpClientOptions, ExchangeError> {
        Err(ExchangeError::new(
            "TxFlow inputs are unavailable on this host",
        ))
    }

    /// Optional hosted account capability; no host-local secret is required.
    fn bitrue(&self) -> Result<safe_cex_venue_bitrue::BitrueDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "bitrue inputs are unavailable on this host",
        ))
    }

    fn hosted_signer(&self) -> Option<HostedSigner> {
        None
    }
    fn weex(&self) -> Result<WeexDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "WEEX dependencies were not supplied by this RustVenueHost",
        ))
    }
    fn woo(&self) -> Result<WooDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "WOO dependencies were not supplied by this RustVenueHost",
        ))
    }
    /// Bybit linear/options and Demo share the same concrete binding inputs.
    fn bybit(&self) -> Result<safe_cex_venue_bybit::BybitDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Bybit inputs are unavailable on this host",
        ))
    }
    fn coinbase(&self) -> Result<CoinbaseDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Coinbase dependencies were not supplied by this RustVenueHost",
        ))
    }
    fn apexomni(&self) -> Result<safe_cex_venue_apexomni::ApexDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "ApexOmni dependencies are unavailable on this host",
        ))
    }
    fn bingx(&self) -> Result<safe_cex_venue_bingx::BingxDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "BingX dependencies are unavailable on this host",
        ))
    }
    /// Lighter and Lighter Robinhood use the same bound class and host services.
    fn lighter(&self) -> Result<LighterDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Lighter dependencies are unavailable on this host",
        ))
    }
    fn breakoutprop(
        &self,
        _options: &ExchangeOptions,
    ) -> Result<safe_cex_venue_breakoutprop::host::BreakoutDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Breakout Prop dependencies are unavailable on this host",
        ))
    }
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError>;
    fn binanceinverse(
        &self,
    ) -> Result<safe_cex_venue_binance::inverse::BinanceInverseDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Binance inverse dependencies were not supplied by this host",
        ))
    }

    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError>;
    /// OKX, OKX Demo, OKX EU and OKX EU Demo share one input set.
    fn okx(&self) -> Result<OkxDependencies, ExchangeError>;
    fn blofin(&self) -> Result<BlofinDependencies, ExchangeError>;
    fn bitunix(&self) -> Result<BitunixDependencies, ExchangeError>;
    /// Futures and Spot share the SDK runtime's nonce and client-ID state.
    fn kraken(&self) -> Result<KrakenDependencies, ExchangeError>;
    /// Bitget, Bitget USDC and Bitget inverse share one input set.
    fn bitget(&self) -> Result<BitgetDependencies, ExchangeError>;
    fn grvt(&self) -> Result<GrvtDependencies, ExchangeError>;
    fn hyperliquid(
        &self,
        name: ExchangeName,
        options: &ExchangeOptions,
    ) -> Result<safe_cex_venue_hyperliquid::binding::HyperliquidDependencies, ExchangeError>;
    fn polymarket(&self) -> Result<PolymarketDependencies, ExchangeError> {
        Err(ExchangeError::new(
            "Polymarket host inputs are not configured",
        ))
    }
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
    (ExchangeName::Apexomni, apexomni_constructor),
    (ExchangeName::Asterdex, asterdex_constructor),
    (ExchangeName::Backpack, backpack_constructor),
    (ExchangeName::Binance, binance_constructor),
    (ExchangeName::Binanceinverse, binanceinverse_constructor),
    (ExchangeName::Binancepm, binancepm_constructor),
    (ExchangeName::Binancespot, binancespot_constructor),
    (ExchangeName::Bingx, bingx_constructor),
    (ExchangeName::Bitget, bitget_constructor),
    (ExchangeName::Bitgetinverse, bitget_constructor),
    (ExchangeName::Bitgetusdc, bitget_constructor),
    (ExchangeName::Bitrue, bitrue_constructor),
    (ExchangeName::Bitunix, bitunix_constructor),
    (ExchangeName::Blofin, blofin_constructor),
    (ExchangeName::Breakoutprop, breakoutprop_constructor),
    (ExchangeName::Grvt, grvt_constructor),
    (ExchangeName::Bybit, bybit_constructor),
    (ExchangeName::Bybitdemo, bybit_constructor),
    (ExchangeName::Bybitspot, bybit_constructor),
    (ExchangeName::Bybitinverse, bybit_constructor),
    (ExchangeName::Hyrotrader, bybit_constructor),
    (ExchangeName::Coinbase, coinbase_constructor),
    (ExchangeName::Coinbasespot, coinbase_constructor),
    (ExchangeName::Cryptocom, cryptocom_constructor),
    (ExchangeName::Hyperliquid, hyperliquid_constructor),
    (ExchangeName::Hyperliquidspot, hyperliquidspot_constructor),
    (ExchangeName::Kraken, kraken_constructor),
    (ExchangeName::Krakenspot, kraken_constructor),
    (ExchangeName::Kodiakfi, orderly_constructor),
    (ExchangeName::Mode, orderly_constructor),
    (ExchangeName::Salsadex, orderly_constructor),
    (ExchangeName::TealstreetDex, orderly_constructor),
    (ExchangeName::What, orderly_constructor),
    (ExchangeName::Woofi, orderly_constructor),
    (ExchangeName::Lighter, lighter_constructor),
    (ExchangeName::LighterRobinhood, lighter_constructor),
    (ExchangeName::Kucoin, kucoin_constructor),
    (ExchangeName::Kucoinspot, kucoin_constructor),
    (ExchangeName::Mexc, mexc_constructor),
    (ExchangeName::Okx, okx_constructor),
    (ExchangeName::Okxdemo, okx_constructor),
    (ExchangeName::Okxeu, okx_constructor),
    (ExchangeName::Okxeudemo, okx_constructor),
    (ExchangeName::Paradex, paradex_constructor),
    (ExchangeName::Phemex, phemex_constructor),
    (ExchangeName::Polymarket, polymarket_constructor),
    (ExchangeName::Txflow, txflow_constructor),
    (ExchangeName::Woo, woo_constructor),
    (ExchangeName::Weex, weex_constructor),
];

pub fn orderly_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.orderly(name.clone())?;
        Ok(
            safe_cex_venue_orderly::native_exchange::OrderlyExchange::new(
                name, options, store, deps,
            )?,
        )
    })
}

pub fn grvt_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(GrvtExchange::new(name, options, store, host.grvt()?)?)
    })
}

pub fn hyperliquidspot_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |_, options, store| {
        let deps = host.hyperliquid(ExchangeName::Hyperliquidspot, &options)?;
        Ok(
            safe_cex_venue_hyperliquid::spot_binding::HyperliquidSpotExchange::new(
                options, store, deps,
            )?,
        )
    })
}

pub fn hyperliquid_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |_, options, store| {
        let deps = host.hyperliquid(ExchangeName::Hyperliquid, &options)?;
        let exchange =
            safe_cex_venue_hyperliquid::exchange::HyperliquidExchange::new(options, store, deps)?;
        Ok(exchange)
    })
}

pub fn kucoin_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.kucoin(name.clone(), &options)?;
        let parts = safe_cex_exchange::js_runtime::base_exchange::BaseExchangeParts {
            name: name.clone(),
            options,
            store,
            settings: ExchangeSettings {
                supports_public_only: true,
                positions_supported: Some(name != ExchangeName::Kucoinspot),
                ..Default::default()
            },
            external_public_data: host.kucoin_external_public_data(),
        };
        match name {
            ExchangeName::Kucoin => Ok(
                safe_cex_venue_kucoin::kucoin_exchange::KucoinExchange::new(parts, deps)?,
            ),
            ExchangeName::Kucoinspot => {
                Ok(safe_cex_venue_kucoin::kucoin_exchange::KucoinSpotExchange::new(parts, deps)?)
            }
            _ => Err(ExchangeError::new(
                "KuCoin constructor requires a KuCoin family name",
            )),
        }
    })
}

pub fn mexc_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.mexc(&options)?;
        let external = external_factory("mexc", deps.external_public_data, &options)?;
        let mut base = BaseExchange::new(
            name,
            options,
            store,
            deps.runtime.clone(),
            ExchangeSettings {
                supports_public_only: true,
                tick_interval: Some(2_000.0),
                ..Default::default()
            },
        );
        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);
        let weak: Rc<
            std::cell::RefCell<std::rc::Weak<safe_cex_venue_mexc::exchange::MexcExchange>>,
        > = Rc::default();
        let fill_parent = weak.clone();
        let venue = safe_cex_venue_mexc::exchange::MexcExchange::new(
            base,
            deps.http,
            deps.local_to_utc,
            safe_cex_venue_mexc::sockets::MexcSocketDependencies {
                current_time_ms: deps.current_time_ms,
                runtime: deps.runtime,
                remote_account_name: host.hosted_signer().and_then(|(_, account)| account),
                emit_fill: Rc::new(move |fill| {
                    use safe_cex_exchange::exchange::Exchange;
                    if let Some(parent) = fill_parent.borrow().upgrade() {
                        parent.base().emit(
                            "fill",
                            safe_cex_exchange::js_runtime::value::from_typed(&fill)?,
                        )?;
                    }
                    Ok(())
                }),
            },
            external,
        )?;
        *weak.borrow_mut() = Rc::downgrade(&venue);
        Ok(venue)
    })
}

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

pub fn phemex_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(
            safe_cex_venue_phemex::binding::exchange::PhemexExchange::new(
                name,
                options,
                store,
                host.phemex()?,
            )?,
        )
    })
}

pub fn cryptocom_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(
            safe_cex_venue_cryptocom::binding::exchange::CryptocomExchange::new(
                name,
                options,
                store,
                host.cryptocom()?,
            )?,
        )
    })
}

pub fn backpack_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.backpack()?;
        let mut base = BaseExchange::new(
            name,
            options,
            store,
            deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = host.hosted_signer().map(|(signer, _)| signer);
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

pub fn binanceinverse_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = safe_cex_venue_binance::inverse::BinanceInverseExchange::new(
            name,
            options,
            store,
            host.binanceinverse()?,
        )?;
        Ok(exchange)
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

pub fn weex_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(WeexExchange::new(name, options, store, host.weex()?)?)
    })
}

pub fn woo_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(WooExchange::new(name, options, store, host.woo()?)?)
    })
}

pub fn bybit_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange =
            safe_cex_venue_bybit::BybitExchange::create(name, options, store, host.bybit()?)?;
        Ok(exchange)
    })
}

pub fn coinbase_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = CoinbaseExchange::new(name, options, store, host.coinbase()?)?;
        Ok(exchange)
    })
}

pub fn bitget_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = BitgetExchange::new(name, options, store, host.bitget()?)?;
        Ok(exchange)
    })
}

pub fn bitrue_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(safe_cex_venue_bitrue::BitrueExchange::new(
            name,
            options,
            store,
            host.bitrue()?,
        )?)
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

pub fn lighter_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(LighterNativeExchange::new(
            name,
            options,
            store,
            host.lighter()?,
        )?)
    })
}

pub fn okx_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let exchange = OkxExchange::new(name, options, store, host.okx()?)?;
        Ok(exchange)
    })
}

pub fn polymarket_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        Ok(PolymarketExchange::new(
            name,
            options,
            store,
            host.polymarket()?,
        )?)
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

pub fn apexomni_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        safe_cex_venue_apexomni::ApexOmniExchange::new(name, options, store, host.apexomni()?)
            .map(|venue| venue as Rc<dyn safe_cex_exchange::exchange::Exchange>)
    })
}

pub fn paradex_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        safe_cex_venue_paradex::native_exchange::ParadexExchange::new(
            name,
            options,
            store,
            host.paradex()?,
        )
        .map(|venue| venue as Rc<dyn safe_cex_exchange::exchange::Exchange>)
    })
}

/// Construct the existing scaffold without widening its Phase-0 capabilities.
pub fn txflow_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let http = host.txflow(&options)?;
        let runtime = http.runtime.clone();
        Ok(Rc::new(
            safe_cex_venue_txflow::exchange::TxFlowExchange::new(
                name,
                options,
                store,
                runtime,
                http,
                &safe_cex_transport::TransportRegistry::default(),
            )?,
        ))
    })
}

/// Construct DXtrade with the same shared core and source API/polling lifecycle.
pub fn breakoutprop_constructor(host: Rc<dyn RustVenueHost>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |name, options, store| {
        let deps = host.breakoutprop(&options)?;
        safe_cex_venue_breakoutprop::lifecycle::BreakoutpropExchange::new(
            safe_cex_venue_breakoutprop::host::ProductionHost::new(deps),
            name,
            options,
            store,
        )
        .map(|exchange| exchange as Rc<dyn safe_cex_exchange::exchange::Exchange>)
    })
}
