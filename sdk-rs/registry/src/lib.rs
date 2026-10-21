//! Every Rust venue, constructed by name from one options shape. Hosts (wasm,
//! native) parse the account options once and receive the venue's engine
//! through a [`Visit`]or; each venue's construction rules live here.
#[cfg(not(any(
    feature = "grvt",
    feature = "paradex",
    feature = "bitrue",
    feature = "coinbase",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
)))]
compile_error!(
    "safe-cex-venues requires at least one venue feature: breakoutprop, paradex, apexomni, asterdex, bitrue, binance, kucoin, kucoinspot, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, grvt, hyperliquid, kraken, txflow, coinbase, lighter, orderly, polymarket, okx, phemex, woo or weex"
);

use safe_cex_engine::{Engine, ErrorKind, Level, PublicFeedPolicy, Venue, VenueError};
#[cfg(feature = "apexomni")]
use safe_cex_venue_apexomni::{ApexOmni, Options as ApexOmniOptions};
#[cfg(feature = "asterdex")]
use safe_cex_venue_asterdex::{AsterDex, Options as AsterOptions};
#[cfg(feature = "backpack")]
use safe_cex_venue_backpack::{Backpack, Options as BackpackOptions};
#[cfg(feature = "binance")]
use safe_cex_venue_binance::{
    BinanceFutures, Options as BinanceOptions, Variant as BinanceVariant,
};
#[cfg(feature = "bingx")]
use safe_cex_venue_bingx::{Bingx, Options as BingxOptions};
#[cfg(feature = "bitget")]
use safe_cex_venue_bitget::{BitgetFutures, Options as BitgetOptions, Product};
#[cfg(feature = "bitrue")]
use safe_cex_venue_bitrue::{Bitrue, Options as BitrueOptions};
#[cfg(feature = "blofin")]
use safe_cex_venue_blofin::{Blofin, Options as BlofinOptions};
#[cfg(feature = "breakoutprop")]
use safe_cex_venue_breakoutprop::{BreakoutProp, Options as BreakoutOptions};
#[cfg(feature = "bybit")]
use safe_cex_venue_bybit::{
    BybitLinear, Endpoints as BybitEndpoints, Options as BybitOptions, Variant as BybitVariant,
};
#[cfg(feature = "coinbase")]
use safe_cex_venue_coinbase::{Coinbase, Options as CoinbaseOptions};
#[cfg(feature = "cryptocom")]
use safe_cex_venue_cryptocom::{Cryptocom, Options as CryptocomOptions};
#[cfg(feature = "grvt")]
use safe_cex_venue_grvt::{Grvt, Options as GrvtOptions};
#[cfg(feature = "hyperliquid")]
use safe_cex_venue_hyperliquid::{Builder, Hyperliquid, Options as HyperliquidOptions};
#[cfg(feature = "kraken")]
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options as KrakenOptions};
#[cfg(feature = "kucoin")]
use safe_cex_venue_kucoin::Kucoin;
#[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
use safe_cex_venue_kucoin::Options as KucoinOptions;
#[cfg(feature = "kucoinspot")]
use safe_cex_venue_kucoin::spot::KucoinSpot;
#[cfg(feature = "lighter")]
use safe_cex_venue_lighter::{Lighter, Options as LighterOptions};
#[cfg(feature = "mexc")]
use safe_cex_venue_mexc::{MexcFutures, Options as MexcOptions};
#[cfg(feature = "okx")]
use safe_cex_venue_okx::{Okx, Options as OkxOptions};
#[cfg(feature = "orderly")]
use safe_cex_venue_orderly::{Broker as OrderlyBroker, Options as OrderlyOptions, Orderly};
#[cfg(feature = "paradex")]
use safe_cex_venue_paradex::{Options as ParadexOptions, Paradex};
#[cfg(feature = "phemex")]
use safe_cex_venue_phemex::{Options as PhemexOptions, PhemexPerpetuals};
#[cfg(feature = "polymarket")]
use safe_cex_venue_polymarket::{
    Options as PolymarketOptions, Polymarket, PolymarketOptions as PolymarketAccountOptions,
};
#[cfg(feature = "weex")]
use safe_cex_venue_weex::{Options as WeexOptions, WeexFutures};
#[cfg(feature = "woo")]
use safe_cex_venue_woo::{Options as WooOptions, Woo};
use serde::Deserialize;

/// Receives the constructed engine; the host decides how to own it.
pub trait Visit {
    type Output;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output;
}

fn invalid(message: impl std::fmt::Display) -> VenueError {
    VenueError::new(message.to_string()).kind(ErrorKind::InvalidInput)
}

/// Engine settings every venue receives after construction.
#[derive(Clone, Copy)]
struct Startup {
    account: bool,
    log_level: Level,
}

fn finish<T: Visit, V: Venue + 'static>(
    visitor: T,
    policy: PublicFeedPolicy,
    startup: Startup,
    mut engine: Engine<V>,
) -> Result<T::Output, VenueError> {
    engine.set_public_feed_policy(policy).map_err(invalid)?;
    engine.set_account_startup(startup.account);
    engine.set_log_level(startup.log_level);
    Ok(visitor.visit(engine))
}

#[cfg(feature = "breakoutprop")]
fn breakoutprop_rules(options: &EngineOptions) -> Result<(), VenueError> {
    // Market data needs a DXtrade session too, so public accounts are not exempt.
    if options.key.as_deref().is_none_or(str::is_empty)
        || options.secret.as_deref().is_none_or(str::is_empty)
    {
        return Err(invalid(
            "BreakoutProp market data requires a legacy DXtrade username and password",
        ));
    }
    Ok(())
}

#[cfg(any(feature = "polymarket", feature = "asterdex"))]
fn hex_of(value: &str, digits: usize) -> bool {
    value.len() == digits && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(feature = "polymarket")]
fn polymarket_rules(options: &EngineOptions) -> Result<(), VenueError> {
    if options.public_data_only {
        return Ok(());
    }
    let key = options.key.as_deref().unwrap_or_default();
    if !hex_of(key.strip_prefix("0x").unwrap_or(key), 64) {
        return Err(invalid(
            "Polymarket requires a local 32-byte wallet private key",
        ));
    }
    let Some(account) = &options.polymarket else {
        return Ok(());
    };
    let credentials = [
        &account.api_key,
        &account.api_secret,
        &account.api_passphrase,
    ];
    if credentials.iter().any(|value| value.is_some())
        && !credentials
            .iter()
            .all(|value| value.as_deref().is_some_and(|text| !text.trim().is_empty()))
    {
        return Err(invalid(
            "Polymarket requires all three L2 API credentials together",
        ));
    }
    if account.signer_type > 3 {
        return Err(invalid("Invalid Polymarket signature type"));
    }
    if (account.signer_type != 0 || account.funder_address.is_some())
        && !account
            .funder_address
            .as_deref()
            .and_then(|address| address.strip_prefix("0x"))
            .is_some_and(|address| hex_of(address, 40))
    {
        return Err(invalid(
            "Polymarket requires a valid funder address for non-EOA signing",
        ));
    }
    Ok(())
}

#[cfg(feature = "orderly")]
fn orderly_rules(options: &EngineOptions) -> Result<(), VenueError> {
    if options.public_data_only {
        return Ok(());
    }
    let application_id = options.application_id.as_deref().unwrap_or_default();
    if application_id.is_empty()
        || !application_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid("Orderly private applicationId is required"));
    }
    if !safe_cex_venue_orderly::signing_keys_valid(
        options.key.as_deref().unwrap_or_default(),
        options.secret.as_deref().unwrap_or_default(),
    ) {
        return Err(invalid(
            "Orderly requires base58-encoded 32-byte key and secret",
        ));
    }
    Ok(())
}

/// The message names the Main Wallet Address field; the app routes on its first sentence.
#[cfg(feature = "asterdex")]
const ASTERDEX_MAIN_WALLET_REQUIRED: &str = "Aster API Wallet/vault keys require the Main Wallet Address. Fill Main Wallet Address with your login wallet — do not replace the main wallet with the API/vault wallet.";

#[cfg(feature = "asterdex")]
fn asterdex_rules(options: &EngineOptions) -> Result<(), VenueError> {
    // V3 wallet credentials under the V1 API mean the user put the API wallet where the main wallet goes.
    let key = options.key.as_deref().unwrap_or_default();
    let secret = options.secret.as_deref().unwrap_or_default();
    if !options.public_data_only
        && !matches!(
            options.asterdex.api_version,
            Some(safe_cex_venue_asterdex::sign::Version::V3)
        )
        && key
            .strip_prefix("0x")
            .is_some_and(|address| hex_of(address, 40))
        && hex_of(secret.strip_prefix("0x").unwrap_or(secret), 64)
        && [&options.asterdex.user_address, &options.application_id]
            .iter()
            .all(|value| value.as_deref().is_none_or(|text| text.trim().is_empty()))
    {
        return Err(invalid(ASTERDEX_MAIN_WALLET_REQUIRED));
    }
    Ok(())
}

/// Constructs `venue`'s engine from `options`, enforcing its construction rules.
pub fn build<T: Visit>(
    venue: &str,
    mut options: EngineOptions,
    visitor: T,
) -> Result<T::Output, VenueError> {
    if let Some(read) = &options.read_credentials {
        read.validate().map_err(invalid)?;
    }
    #[cfg(feature = "weex")]
    if venue == "weex"
        && !options.public_data_only
        && options.passphrase.as_deref().is_none_or(str::is_empty)
    {
        return Err(invalid("WEEX requires a passphrase"));
    }
    #[cfg(feature = "kraken")]
    if matches!(venue, "kraken" | "krakenspot")
        && !options.public_data_only
        && options.key.as_deref().is_some_and(|key| !key.is_empty())
        && let Some(secret) = options.secret.as_deref()
    {
        safe_cex_venue_kraken::sign::Signer::new(secret).map_err(invalid)?;
    }
    let policy = options
        .public_feed
        .as_ref()
        .map(|feed| feed.mode)
        .unwrap_or(if options.private_only {
            PublicFeedPolicy::Disabled
        } else {
            PublicFeedPolicy::Direct
        });
    if options.private_only && policy == PublicFeedPolicy::Direct {
        return Err(invalid("privateOnly cannot use direct public feeds"));
    }
    let startup = Startup {
        account: options.account_startup,
        log_level: match options.log_level.as_deref() {
            None => Level::Info,
            Some(text) => Level::parse(text)
                .ok_or_else(|| invalid("logLevel must be debug, info, warn or error"))?,
        },
    };
    match venue {
        #[cfg(feature = "breakoutprop")]
        "breakoutprop" => {
            breakoutprop_rules(&options)?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(BreakoutProp::new(BreakoutOptions {
                    key: options.key.take().unwrap_or_default(),
                    secret: options.secret.take().unwrap_or_default(),
                    subaccount: options.subaccount.take(),
                    public_data_only: options.public_data_only,
                    tick_interval: if options.polling_enabled == Some(false) {
                        f64::INFINITY
                    } else {
                        safe_cex_engine::engine::bounded_tick_interval(
                            options.tick_interval.take().unwrap_or(2_000.0),
                        )
                    },
                    timeout_ms: options
                        .recv_window
                        .unwrap_or(20_000.0)
                        .clamp(1.0, u32::MAX as f64) as u32,
                })),
            )
        }
        #[cfg(feature = "grvt")]
        "grvt" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Grvt::new(GrvtOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                trading_account_id: options.grvt_trading_account_id.take(),
                signer_address: options.grvt_signer_address.take(),
                session_proxy: options.grvt_session_proxy,
            })),
        ),
        #[cfg(feature = "coinbase")]
        "coinbase" | "coinbasespot" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                Coinbase::new(CoinbaseOptions {
                    key: options.key.take().unwrap_or_default(),
                    secret: options.secret.take().unwrap_or_default(),
                    public_data_only: options.public_data_only,
                    spot_only: venue == "coinbasespot" || options.spot_only,
                    testnet: options.testnet,
                    read_credentials: options.read_credentials.clone(),
                })
                .map_err(invalid)?,
            ),
        ),
        #[cfg(feature = "polymarket")]
        "polymarket" => {
            polymarket_rules(&options)?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(Polymarket::new(PolymarketOptions {
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    polymarket: options.polymarket.take(),
                })),
            )
        }
        #[cfg(feature = "paradex")]
        "paradex" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Paradex::new(ParadexOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                paradex_address: options.paradex_address.take(),
                subkey_address: options.subkey_address.take(),
                payload_signer: options.payload_signer,
            })),
        ),
        #[cfg(feature = "apexomni")]
        "apexomni" => finish(
            visitor,
            policy,
            startup,
            Engine::new(ApexOmni::new(ApexOmniOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                passphrase: options.passphrase.take(),
                omni_key_seed: options.omni_key_seed.take(),
                tick_interval_ms: None,
            })),
        ),
        #[cfg(feature = "txflow")]
        "txflow" => {
            let mut options = safe_cex_venue_txflow::Options {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                application_id: options.application_id.take(),
                vault: options.vault.take(),
            };
            options.validate().map_err(|error| {
                use zeroize::Zeroize;
                options.key.zeroize();
                options.secret.zeroize();
                invalid(&error)
            })?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(safe_cex_venue_txflow::TxFlow::new(options)),
            )
        }
        #[cfg(feature = "kraken")]
        "kraken" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Kraken::new(KrakenOptions {
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "kraken")]
        "krakenspot" => finish(
            visitor,
            policy,
            startup,
            Engine::new(KrakenSpot::new(KrakenOptions {
                // Spot private reads are POSTs and retain primary credentials.
                read_credentials: None,
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "asterdex")]
        "asterdex" => {
            asterdex_rules(&options)?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(AsterDex::new(AsterOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    application_id: options.application_id.take(),
                    auth_user_address: options.asterdex.user_address.take(),
                    auth_signer_address: options.asterdex.signer_address.take(),
                    auth_private_key: options.asterdex.private_key.take(),
                    api_version: options.asterdex.api_version,
                    builder_address: options.asterdex.builder_address.take(),
                    builder_fee_rate_maker: options.asterdex.builder_fee_rate_maker.take(),
                    builder_fee_rate_taker: options.asterdex.builder_fee_rate_taker.take(),
                    read_key: options.asterdex.read_key.take(),
                    read_secret: options.asterdex.read_secret.take(),
                })),
            )
        }
        #[cfg(feature = "orderly")]
        "orderly" | "tealstreet-dex" | "kodiakfi" | "mode" | "salsadex" | "what" | "woofi" => {
            orderly_rules(&options)?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(Orderly::new(OrderlyOptions {
                    broker: match venue {
                        "kodiakfi" => OrderlyBroker::KodiakFi,
                        "mode" => OrderlyBroker::Mode,
                        "salsadex" => OrderlyBroker::SalsaDex,
                        "what" => OrderlyBroker::What,
                        "woofi" => OrderlyBroker::WooFi,
                        _ => OrderlyBroker::Tealstreet,
                    },
                    public_data_only: options.public_data_only,
                    testnet: options.testnet,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    application_id: options.application_id.take(),
                    tick_interval: options
                        .tick_interval
                        .take()
                        .map(safe_cex_engine::engine::bounded_tick_interval),
                })),
            )
        }
        #[cfg(feature = "lighter")]
        "lighter" | "lighter-robinhood" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Lighter::new(LighterOptions {
                robinhood: venue == "lighter-robinhood",
                remote_signer: options.lighter_remote_signer,
                signer_provider: options.lighter_signer_provider.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                application_id: options.application_id.take(),
                api_key_index: options.api_key_index.take(),
                integrator_account_index: options.lighter_integrator_account_index.take(),
                stop_slippage_pct: options.lighter_stop_slippage_pct.take().unwrap_or(1.0),
            })),
        ),
        #[cfg(feature = "bitrue")]
        "bitrue" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Bitrue::new(BitrueOptions {
                read_credentials: options.read_credentials.take(),
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                recv_window: options
                    .recv_window
                    .take()
                    .unwrap_or(20_000.0)
                    .clamp(1.0, 60_000.0) as u32,
            })),
        ),
        #[cfg(feature = "backpack")]
        "backpack" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Backpack::new(BackpackOptions {
                read_credentials: options.read_credentials.take(),
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                recv_window: options.recv_window.take(),
            })),
        ),
        #[cfg(feature = "blofin")]
        "blofin" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Blofin::new(BlofinOptions {
                broker_attribution: options.broker_attribution,
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                passphrase: options.passphrase.take(),
            })),
        ),
        #[cfg(feature = "mexc")]
        "mexc" => finish(
            visitor,
            policy,
            startup,
            Engine::new(MexcFutures::new(MexcOptions {
                recv_window: tighter_recv_window(options.recv_window),
                read_credentials: options.read_credentials.take(),
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "phemex")]
        "phemex" => finish(
            visitor,
            policy,
            startup,
            Engine::new(PhemexPerpetuals::new(PhemexOptions {
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "binance")]
        "binance" | "binancepm" | "binancespot" | "binanceinverse" => finish(
            visitor,
            policy,
            startup,
            Engine::new(BinanceFutures::new(BinanceOptions {
                recv_window: options.recv_window,
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                portfolio_margin: venue == "binancepm",
                variant: match venue {
                    "binancepm" => BinanceVariant::PortfolioMargin,
                    "binancespot" => BinanceVariant::Spot,
                    "binanceinverse" => BinanceVariant::CoinM,
                    _ => BinanceVariant::UsdM,
                },
            })),
        ),
        #[cfg(feature = "kucoinspot")]
        "kucoinspot" => finish(
            visitor,
            policy,
            startup,
            Engine::new(KucoinSpot::new(KucoinOptions {
                read_credentials: options.read_credentials.take(),
                broker: options.extra.take().and_then(|e| e.kucoin_broker),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                passphrase: options.passphrase.take(),
            })),
        ),
        #[cfg(feature = "kucoin")]
        "kucoin" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Kucoin::new(KucoinOptions {
                read_credentials: options.read_credentials.take(),
                broker: options.extra.take().and_then(|e| e.kucoin_broker),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                passphrase: options.passphrase.take(),
            })),
        ),
        #[cfg(feature = "bingx")]
        "bingx" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Bingx::new(BingxOptions {
                read_credentials: options.read_credentials.take(),
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "weex")]
        "weex" => finish(
            visitor,
            policy,
            startup,
            Engine::new(WeexFutures::new(WeexOptions {
                read_credentials: options.read_credentials.take(),
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                passphrase: options.passphrase.take(),
                private_ws_headers: options.private_ws_headers,
            })),
        ),
        #[cfg(feature = "woo")]
        "woo" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Woo::new(WooOptions {
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                application_id: options.application_id.take(),
            })),
        ),
        #[cfg(feature = "bybit")]
        "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => {
            let mut options = BybitOptions {
                recv_window: tighter_recv_window(options.recv_window),
                read_credentials: options.read_credentials.take(),
                enable_inverse_markets: options.enable_inverse_markets,
                enable_option_markets: options.enable_option_markets,
                variant: match venue {
                    "bybitinverse" => BybitVariant::Inverse,
                    "bybitspot" => BybitVariant::Spot,
                    "bybitdemo" => BybitVariant::Demo,
                    "hyrotrader" => BybitVariant::Hyrotrader,
                    _ => BybitVariant::Linear,
                },
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                endpoints: options.endpoints.take().map(|e| BybitEndpoints {
                    rest: e.rest,
                    public_ws: e.public_ws,
                    private_ws: e.private_ws,
                }),
            };
            options.validate().map_err(|error| {
                use zeroize::Zeroize;
                options.key.zeroize();
                options.secret.zeroize();
                if let Some(read) = &mut options.read_credentials {
                    read.erase_secrets();
                }
                invalid(&error)
            })?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(BybitLinear::new(options)),
            )
        }
        #[cfg(feature = "bitget")]
        "bitget" | "bitgetusdc" | "bitgetinverse" => {
            if !options.public_data_only
                && options.key.as_ref().is_some_and(|s| !s.is_empty())
                && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
            {
                return Err(invalid("Bitget requires a passphrase"));
            }
            finish(
                visitor,
                policy,
                startup,
                Engine::new(BitgetFutures::new(BitgetOptions {
                    read_credentials: options.read_credentials.take(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    passphrase: options.passphrase.take(),
                    product: if venue == "bitgetinverse" {
                        Product::Inverse
                    } else if venue == "bitgetusdc" {
                        Product::Usdc
                    } else {
                        Product::Usdt
                    },
                })),
            )
        }
        #[cfg(feature = "cryptocom")]
        "cryptocom" => finish(
            visitor,
            policy,
            startup,
            Engine::new(Cryptocom::new(CryptocomOptions {
                read_credentials: options.read_credentials.take(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
            })),
        ),
        #[cfg(feature = "hyperliquid")]
        "hyperliquid" | "hyperliquidspot" => {
            let config = HyperliquidOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                secret: options.secret.take(),
                application_id: options.application_id.take(),
                vault: options.vault.take(),
                builder: options.hyperliquid_builder.take().map(|b| Builder {
                    b: b.address,
                    f: b.fee_tenths_bps,
                }),
                enable_spot_markets: venue == "hyperliquid" && options.enable_spot_markets,
            };
            finish(
                visitor,
                policy,
                startup,
                Engine::new(if venue == "hyperliquidspot" {
                    Hyperliquid::new_spot(config)
                } else {
                    Hyperliquid::new(config)
                }),
            )
        }
        #[cfg(feature = "okx")]
        "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
            if !options.public_data_only
                && options.key.as_ref().is_some_and(|s| !s.is_empty())
                && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
            {
                return Err(invalid("OKX requires a passphrase"));
            }
            finish(
                visitor,
                policy,
                startup,
                Engine::new(Okx::new(OkxOptions {
                    read_credentials: options.read_credentials.take(),
                    testnet: options.testnet || venue.ends_with("demo"),
                    eea: venue.starts_with("okxeu"),
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    passphrase: options.passphrase.take(),
                })),
            )
        }
        other => Err(invalid(format!("unsupported venue: {other}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFeedOptions {
    pub mode: safe_cex_engine::PublicFeedPolicy,
}

#[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KucoinExtra {
    #[serde(default)]
    pub kucoin_broker: Option<safe_cex_venue_kucoin::sign::BrokerOptions>,
}

#[cfg(any(
    feature = "coinbase",
    feature = "paradex",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "grvt",
    feature = "bitrue",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineOptions {
    #[serde(default)]
    pub private_only: bool,
    /// The SDK sets this when the account can sign (not public-data-only).
    #[serde(default)]
    pub account_startup: bool,
    #[serde(default)]
    pub public_feed: Option<PublicFeedOptions>,
    /// Minimum event level: `debug`, `info` (default), `warn` or `error`.
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    #[cfg(feature = "blofin")]
    #[serde(default)]
    pub broker_attribution: bool,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    pub enable_inverse_markets: bool,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    pub enable_option_markets: bool,
    #[cfg(feature = "coinbase")]
    #[serde(default)]
    pub spot_only: bool,
    #[cfg(feature = "polymarket")]
    #[serde(default)]
    pub polymarket: Option<PolymarketAccountOptions>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    pub api_key_index: Option<String>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    #[serde(deserialize_with = "optional_lighter_index")]
    pub lighter_integrator_account_index: Option<u64>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    pub lighter_stop_slippage_pct: Option<f64>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    pub lighter_remote_signer: bool,
    #[cfg(feature = "lighter")]
    pub lighter_signer_provider: Option<String>,
    #[cfg(any(
        feature = "grvt",
        feature = "coinbase",
        feature = "paradex",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "blofin",
        feature = "bybit",
        feature = "woo",
        feature = "cryptocom",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "txflow",
        feature = "phemex",
        feature = "orderly",
        feature = "lighter",
        feature = "bitget",
        feature = "okx"
    ))]
    #[serde(default)]
    pub testnet: bool,
    #[serde(default)]
    pub public_data_only: bool,
    #[serde(default)]
    pub key: Option<String>,
    #[cfg(any(
        feature = "grvt",
        feature = "coinbase",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "breakoutprop",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "blofin",
        feature = "bybit",
        feature = "cryptocom",
        feature = "bingx",
        feature = "mexc",
        feature = "bitget",
        feature = "bitrue",
        feature = "backpack",
        feature = "phemex",
        feature = "weex",
        feature = "hyperliquid",
        feature = "txflow",
        feature = "kraken",
        feature = "lighter",
        feature = "woo",
        feature = "orderly",
        feature = "okx"
    ))]
    #[serde(default)]
    pub secret: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    pub paradex_address: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    pub subkey_address: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    pub payload_signer: bool,
    #[cfg(any(
        feature = "backpack",
        feature = "bitrue",
        feature = "breakoutprop",
        feature = "binance",
        feature = "bybit",
        feature = "mexc"
    ))]
    #[serde(default)]
    pub recv_window: Option<f64>,
    #[cfg(any(
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "okx",
        feature = "weex",
        feature = "blofin",
        feature = "bitget",
        feature = "apexomni"
    ))]
    #[serde(default)]
    pub passphrase: Option<String>,
    #[cfg(feature = "apexomni")]
    #[serde(default)]
    pub omni_key_seed: Option<String>,
    #[cfg(any(
        feature = "hyperliquid",
        feature = "txflow",
        feature = "asterdex",
        feature = "orderly",
        feature = "lighter",
        feature = "woo"
    ))]
    #[serde(default)]
    pub application_id: Option<String>,
    #[cfg(any(feature = "hyperliquid", feature = "txflow"))]
    #[serde(default)]
    pub vault: Option<String>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    pub hyperliquid_builder: Option<HyperliquidBuilder>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    pub enable_spot_markets: bool,
    #[cfg(feature = "asterdex")]
    #[serde(default)]
    pub asterdex: AsterConfig,

    #[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
    #[serde(default)]
    pub extra: Option<KucoinExtra>,
    #[cfg(feature = "weex")]
    #[serde(default)]
    pub private_ws_headers: bool,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    pub grvt_trading_account_id: Option<String>,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    pub grvt_signer_address: Option<String>,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    pub grvt_session_proxy: bool,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    pub endpoints: Option<EndpointOptions>,
    #[cfg(any(feature = "orderly", feature = "breakoutprop"))]
    #[serde(default)]
    pub tick_interval: Option<f64>,
    #[cfg(feature = "breakoutprop")]
    #[serde(default)]
    pub subaccount: Option<String>,
    #[cfg(feature = "breakoutprop")]
    #[serde(default)]
    pub polling_enabled: Option<bool>,
}
#[cfg(any(
    feature = "coinbase",
    feature = "paradex",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "grvt",
    feature = "bitrue",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
))]
impl EngineOptions {
    pub fn erase_secrets(&mut self) {
        use zeroize::Zeroize;
        self.key.zeroize();
        #[cfg(any(
            feature = "grvt",
            feature = "coinbase",
            feature = "binance",
            feature = "apexomni",
            feature = "asterdex",
            feature = "breakoutprop",
            feature = "kucoin",
            feature = "kucoinspot",
            feature = "blofin",
            feature = "bybit",
            feature = "cryptocom",
            feature = "bingx",
            feature = "mexc",
            feature = "bitget",
            feature = "bitrue",
            feature = "backpack",
            feature = "phemex",
            feature = "weex",
            feature = "hyperliquid",
            feature = "txflow",
            feature = "kraken",
            feature = "lighter",
            feature = "woo",
            feature = "orderly",
            feature = "okx"
        ))]
        self.secret.zeroize();
        #[cfg(any(
            feature = "kucoin",
            feature = "kucoinspot",
            feature = "okx",
            feature = "weex",
            feature = "blofin",
            feature = "bitget",
            feature = "apexomni"
        ))]
        self.passphrase.zeroize();
        #[cfg(feature = "apexomni")]
        self.omni_key_seed.zeroize();
        if let Some(read) = &mut self.read_credentials {
            read.erase_secrets();
        }
        #[cfg(feature = "asterdex")]
        self.asterdex.erase_secrets();
        #[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
        if let Some(broker) = self
            .extra
            .as_mut()
            .and_then(|extra| extra.kucoin_broker.as_mut())
        {
            broker.broker_key.zeroize();
        }
        #[cfg(feature = "polymarket")]
        if let Some(account) = &mut self.polymarket {
            account.api_key.zeroize();
            account.api_secret.zeroize();
            account.api_passphrase.zeroize();
        }
    }
}
#[cfg(any(
    feature = "coinbase",
    feature = "paradex",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "grvt",
    feature = "bitrue",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
))]
impl Drop for EngineOptions {
    fn drop(&mut self) {
        self.erase_secrets();
    }
}

#[cfg(feature = "asterdex")]
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AsterConfig {
    pub api_version: Option<safe_cex_venue_asterdex::sign::Version>,
    pub user_address: Option<String>,
    pub signer_address: Option<String>,
    pub private_key: Option<String>,
    pub builder_address: Option<String>,
    pub builder_fee_rate_maker: Option<String>,
    pub builder_fee_rate_taker: Option<String>,
    pub read_key: Option<String>,
    pub read_secret: Option<String>,
}
#[cfg(feature = "asterdex")]
impl AsterConfig {
    pub fn erase_secrets(&mut self) {
        use zeroize::Zeroize;
        self.private_key.zeroize();
        self.read_key.zeroize();
        self.read_secret.zeroize();
    }
}
#[cfg(feature = "asterdex")]
impl Drop for AsterConfig {
    fn drop(&mut self) {
        self.erase_secrets();
    }
}

#[cfg(feature = "bybit")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointOptions {
    pub rest: String,
    pub public_ws: String,
    pub private_ws: String,
}
#[cfg(feature = "hyperliquid")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidBuilder {
    pub address: String,
    pub fee_tenths_bps: u32,
}

#[cfg(feature = "lighter")]
fn optional_lighter_index<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Index {
        Number(u64),
        Text(String),
    }
    Option::<Index>::deserialize(d)?
        .map(|index| match index {
            Index::Number(n) => Ok(n),
            Index::Text(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        })
        .transpose()
}

#[cfg(any(feature = "bybit", feature = "mexc"))]
fn tighter_recv_window(value: Option<f64>) -> Option<u32> {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.floor().clamp(1.0, 20_000.0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "bybit", feature = "mexc"))]
    #[test]
    fn tighter_receive_windows_survive_the_host_boundary() {
        assert_eq!(tighter_recv_window(Some(5_000.0)), Some(5_000));
        assert_eq!(tighter_recv_window(Some(5_000.9)), Some(5_000));
        assert_eq!(tighter_recv_window(Some(1e300)), Some(20_000));
        assert_eq!(tighter_recv_window(Some(0.5)), Some(1));
        for value in [None, Some(0.0), Some(-1.0), Some(f64::NAN)] {
            assert_eq!(tighter_recv_window(value), None);
        }
    }

    #[cfg(feature = "binance")]
    #[test]
    fn unused_and_rejected_engine_options_have_owned_secret_cleanup() {
        let mut options: EngineOptions = serde_json::from_str(
            r#"{"key":"primary-key","secret":"primary-secret","readCredentials":{"key":"read-key","secret":"read-secret","passphrase":"read-pass"}}"#,
        ).unwrap();
        assert!(std::mem::needs_drop::<EngineOptions>());
        options.erase_secrets();
        assert!(options.key.as_deref().is_none_or(str::is_empty));
        assert!(options.secret.as_deref().is_none_or(str::is_empty));
        let read = options.read_credentials.as_ref().unwrap();
        assert!(read.key.is_empty());
        assert!(read.secret.is_empty());
        assert!(read.passphrase.as_deref().is_none_or(str::is_empty));
        #[cfg(feature = "asterdex")]
        {
            let mut nested: EngineOptions = serde_json::from_str(
                r#"{"asterdex":{"privateKey":"wallet-secret","readKey":"api-key","readSecret":"api-secret"}}"#,
            ).unwrap();
            nested.erase_secrets();
            assert!(
                nested
                    .asterdex
                    .private_key
                    .as_deref()
                    .is_none_or(str::is_empty)
            );
            assert!(
                nested
                    .asterdex
                    .read_key
                    .as_deref()
                    .is_none_or(str::is_empty)
            );
            assert!(
                nested
                    .asterdex
                    .read_secret
                    .as_deref()
                    .is_none_or(str::is_empty)
            );
        }
    }

    #[test]
    fn secondary_read_options_decode_without_changing_primary_identity() {
        let options: EngineOptions = serde_json::from_str(r#"{"key":"primary","secret":"primary-secret","readCredentials":{"key":"read","secret":"read-secret","passphrase":"read-pass"}}"#).unwrap();
        assert_eq!(options.key.as_deref(), Some("primary"));
        let read = options.read_credentials.as_ref().unwrap();
        assert_eq!(read.key, "read");
        assert_eq!(read.secret, "read-secret");
        assert_eq!(read.passphrase.as_deref(), Some("read-pass"));
        assert!(
            serde_json::from_str::<EngineOptions>(r#"{"readCredentials":{"key":"read"}}"#).is_err()
        );
    }
}
