//! Every Rust venue, constructed by name from one options shape. Hosts (wasm,
//! native) parse the account options once and receive the venue's engine
//! through a [`Visit`]or; each venue's construction rules live here.
#[cfg(feature = "mexc")]
mod mexc_setup;
#[cfg(not(any(
    feature = "grvt",
    feature = "paradex",
    feature = "bitrue",
    feature = "coinbase",
    feature = "binance",
    feature = "bitunix",
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
    "safe-cex-venues requires at least one venue feature: breakoutprop, paradex, apexomni, asterdex, bitrue, binance, bitunix, kucoin, kucoinspot, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, grvt, hyperliquid, kraken, txflow, coinbase, lighter, orderly, polymarket, okx, phemex, woo or weex"
);

#[cfg(feature = "kraken")]
pub use safe_cex_venue_kraken::sign::dispatch::Selected as kraken_selected;
#[cfg(feature = "kraken")]
pub use safe_cex_venue_kraken::sign::hosted as kraken_hosted;
#[cfg(feature = "asterdex")]
mod asterdex_setup;
mod construction;
#[cfg(feature = "hosted-signing")]
pub mod hosted_signing;
#[cfg(feature = "hosted-wallet")]
pub mod hosted_wallet;
#[cfg(feature = "hyperliquid")]
mod hyperliquid_setup;
#[cfg(feature = "lighter")]
mod lighter_setup;
#[cfg(feature = "paradex")]
mod paradex_setup;
pub use construction::{
    BridgeConstruction, PublicMode, remote_signing_policy, signer_setup_policy,
    signer_setup_public_data_only,
};
#[cfg(feature = "bybit")]
mod hyro;
#[cfg(feature = "bybit")]
pub use hyro::HyroConfig;

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
#[cfg(feature = "bitunix")]
use safe_cex_venue_bitunix::{BitunixFutures, Options as BitunixOptions};
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
use serde::{Deserialize, Serialize};

/// Resolved, non-sensitive account construction choices, available before engine start.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Construction {
    pub bridge: BridgeConstruction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_sign: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_data_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asterdex_api_version: Option<&'static str>,
    /// Requested frame-fed (`EngineOptions::frame_fed`); omitted when false.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub frame_fed: bool,
}

impl EngineOptions {
    /// Refuses an injected identity without a signing ref while options are
    /// parsed, so the host keeps its existing "Invalid engine options" terminal.
    fn injected_identities<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<safe_cex_engine::signing::Identities>, D::Error> {
        let identities = Option::<safe_cex_engine::signing::Identities>::deserialize(deserializer)?;
        if identities.as_ref().is_some_and(|ids| {
            [ids.primary.as_ref(), ids.secondary.as_ref()]
                .into_iter()
                .flatten()
                .any(|id| id.signing_ref().is_err())
        }) {
            return Err(serde::de::Error::custom(
                "injected identity without a signing ref",
            ));
        }
        Ok(identities)
    }

    pub fn construction(&self, venue: &str) -> Construction {
        let asterdex_api_version = None;
        #[cfg(feature = "asterdex")]
        let asterdex_api_version = if venue == "asterdex" {
            Some(match self.aster_api_version() {
                safe_cex_venue_asterdex::sign::Version::V1 => "v1",
                safe_cex_venue_asterdex::sign::Version::V3 => "v3",
            })
        } else {
            asterdex_api_version
        };
        let _ = venue;
        let mode = construction::resolve(venue, self);
        Construction {
            bridge: construction::bridge(venue, self),
            asterdex_api_version,
            can_sign: mode.as_ref().map(|mode| mode.can_sign),
            public_data_only: mode.map(|mode| mode.public_data_only),
            frame_fed: self.frame_fed,
        }
    }

    #[cfg(feature = "asterdex")]
    fn aster_api_version(&self) -> safe_cex_venue_asterdex::sign::Version {
        self.aster_version(self.asterdex.api_version)
    }

    #[cfg(feature = "asterdex")]
    fn aster_version(
        &self,
        configured: Option<safe_cex_venue_asterdex::sign::Version>,
    ) -> safe_cex_venue_asterdex::sign::Version {
        fn nonempty(value: &Option<String>) -> Option<&str> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
        }
        safe_cex_venue_asterdex::sign::resolve_version(
            configured,
            nonempty(&self.asterdex.user_address).or_else(|| nonempty(&self.application_id)),
            nonempty(&self.asterdex.signer_address).or_else(|| nonempty(&self.key)),
            nonempty(&self.asterdex.private_key).or_else(|| nonempty(&self.secret)),
        )
    }
}

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
    frame_fed: bool,
    log_level: Level,
    private_feed: safe_cex_engine::signing_policy::PrivateFeedPolicy,
}

fn finish<T: Visit, V: Venue + 'static>(
    visitor: T,
    policy: PublicFeedPolicy,
    startup: Startup,
    mut engine: Engine<V>,
) -> Result<T::Output, VenueError> {
    engine.set_public_feed_policy(policy).map_err(invalid)?;
    engine.set_private_feed_policy(startup.private_feed)?;
    engine.set_account_startup(startup.account);
    if startup.frame_fed {
        engine.set_frame_fed().map_err(invalid)?;
    }
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

#[cfg(feature = "polymarket")]
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

/// Backpack's provider copy of the configured key and Ed25519 seed (option B).
/// A probe signature refuses a malformed or non-32-byte seed before startup.
#[cfg(feature = "backpack")]
struct BackpackSignerSetup {
    provider: Option<safe_cex_engine::signing::Provider>,
    identities: safe_cex_engine::signing::Identities,
    read_status: Option<Result<(), safe_cex_venue_backpack::sign::ReadCredentialsError>>,
}

#[cfg(feature = "backpack")]
fn backpack_provider(options: &EngineOptions) -> Result<BackpackSignerSetup, VenueError> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{Algorithm, Identities, KeyEncoding, Operation, PayloadPart};
    let present = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    // Validation metadata survives public-only setup; no signing material does.
    let read_status = options.read_credentials.as_ref().map(|read| {
        safe_cex_venue_backpack::sign::validate_read_credentials(read.validate(), &read.secret)
    });
    // Declared PublicRead keeps a valid read key signing in public-only mode,
    // with no primary ever built or probed (docs/plans/RUST_BACKPACK_PUBLIC_READ_PROVIDER.md).
    // Backpack's own provider (dual custody) applies the one shared rule.
    let read_policy = construction::read_credentials_policy("backpack");
    let primary_allowed = !public_read_excludes_primary(read_policy, options.public_data_only);
    if options.public_data_only && read_policy != construction::ReadCredentialsPolicy::PublicRead {
        return Ok(BackpackSignerSetup {
            provider: None,
            identities: Identities::default(),
            read_status,
        });
    }
    let undecodable = || invalid("Invalid Backpack base64 seed");
    let wrong_width = || invalid("Backpack Ed25519 seed must be 32 bytes");
    let primary =
        (primary_allowed && present(&options.key) && present(&options.secret)).then(|| {
            LocalIdentity {
                key: Secret::new(options.key.clone().unwrap_or_default()),
                secret: Secret::new(options.secret.clone().unwrap_or_default()),
                secret_encoding: KeyEncoding::Base64,
                ..Default::default()
            }
        });
    let secondary = options
        .read_credentials
        .as_ref()
        .filter(|_| matches!(read_status, Some(Ok(()))))
        .map(|read| LocalIdentity {
            key: Secret::new(read.key.clone()),
            secret: Secret::new(read.secret.clone()),
            secret_encoding: KeyEncoding::Base64,
            ..Default::default()
        });
    if primary.is_none() && secondary.is_none() {
        return Ok(BackpackSignerSetup {
            provider: None,
            identities: Identities::default(),
            read_status,
        });
    }
    // Both seeds are Ed25519 keys, declared so the provider signs only Ed25519.
    let ed25519 = Some(safe_cex_engine::signing::IdentitySetupPolicy {
        signing_algorithm: Some(Algorithm::Ed25519),
        ..Default::default()
    });
    let policy = safe_cex_engine::signing::CredentialSetupPolicy {
        primary: ed25519,
        secondary: ed25519,
    };
    let (signer, identities) =
        LocalSigner::from_setup_with_policy(LocalSetup { primary, secondary }, policy)
            .map_err(|_| undecodable())?;
    if let Some(primary) = identities.primary.as_ref() {
        let probe = safe_cex_engine::signing::SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: primary.signing_ref().map_err(|_| wrong_width())?.clone(),
                payload: vec![PayloadPart::Bytes { bytes: Vec::new() }],
            }],
        };
        signer.sign_now(&probe).map_err(|_| wrong_width())?;
    }
    Ok(BackpackSignerSetup {
        provider: Some(std::sync::Arc::new(signer)),
        identities,
        read_status,
    })
}

/// Orderly's one provider setup: the Ed25519 seed is installed as declared
/// base58 and signs every private read and the socket auth. The presented key
/// is native's `ed25519:`-prefixed public key; either string may carry the prefix.
#[cfg(feature = "orderly")]
fn orderly_signer(
    options: &EngineOptions,
) -> Result<
    (
        Option<safe_cex_engine::signing::Provider>,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    let unprefixed = |value: &Option<String>| {
        let value = value.as_deref().unwrap_or_default();
        value.strip_prefix("ed25519:").unwrap_or(value).to_owned()
    };
    let key = unprefixed(&options.key);
    if options.public_data_only || key.is_empty() {
        return Ok((None, Default::default()));
    }
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(format!("ed25519:{key}")),
                secret: Secret::new(unprefixed(&options.secret)),
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base58),
                signing_algorithm: Some(Algorithm::Ed25519),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .map_err(|_| invalid("Invalid Orderly signer setup"))?;
    Ok((Some(std::sync::Arc::new(signer)), identities))
}

/// Whether this build constructs `venue`.
pub fn supported(venue: &str) -> bool {
    match venue {
        #[cfg(feature = "breakoutprop")]
        "breakoutprop" => true,
        #[cfg(feature = "grvt")]
        "grvt" => true,
        #[cfg(feature = "coinbase")]
        "coinbase" | "coinbasespot" => true,
        #[cfg(feature = "polymarket")]
        "polymarket" => true,
        #[cfg(feature = "paradex")]
        "paradex" => true,
        #[cfg(feature = "apexomni")]
        "apexomni" => true,
        #[cfg(feature = "txflow")]
        "txflow" => true,
        #[cfg(feature = "kraken")]
        "kraken" => true,
        #[cfg(feature = "kraken")]
        "krakenspot" => true,
        #[cfg(feature = "asterdex")]
        "asterdex" => true,
        #[cfg(feature = "orderly")]
        "orderly" | "tealstreet-dex" | "kodiakfi" | "mode" | "salsadex" | "what" | "woofi" => true,
        #[cfg(feature = "lighter")]
        "lighter" | "lighter-robinhood" => true,
        #[cfg(feature = "bitrue")]
        "bitrue" => true,
        #[cfg(feature = "bitunix")]
        "bitunix" => true,
        #[cfg(feature = "backpack")]
        "backpack" => true,
        #[cfg(feature = "blofin")]
        "blofin" => true,
        #[cfg(feature = "mexc")]
        "mexc" => true,
        #[cfg(feature = "phemex")]
        "phemex" => true,
        #[cfg(feature = "binance")]
        "binance" | "binancepm" | "binancespot" | "binanceinverse" | "binanceoptions" => true,
        #[cfg(feature = "kucoinspot")]
        "kucoinspot" => true,
        #[cfg(feature = "kucoin")]
        "kucoin" => true,
        #[cfg(feature = "bingx")]
        "bingx" => true,
        #[cfg(feature = "weex")]
        "weex" => true,
        #[cfg(feature = "woo")]
        "woo" => true,
        #[cfg(feature = "bybit")]
        "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => true,
        #[cfg(feature = "bitget")]
        "bitget" | "bitgetusdc" | "bitgetinverse" => true,
        #[cfg(feature = "cryptocom")]
        "cryptocom" => true,
        #[cfg(feature = "hyperliquid")]
        "hyperliquid" | "hyperliquidspot" => true,
        #[cfg(feature = "okx")]
        "okx" | "okxeu" | "okxdemo" | "okxeudemo" => true,
        _ => false,
    }
}

/// Venues that sign with a local key and secret refuse a remote signer in place of one;
/// the rest delegate or derive signing.
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
fn local_signing_rules(venue: &str, options: &EngineOptions) -> Result<(), VenueError> {
    const DELEGATED: [&str; 9] = [
        "hyperliquid",
        "paradex",
        "lighter",
        "lighter-robinhood",
        "txflow",
        "coinbase",
        "coinbasespot",
        "polymarket",
        "breakoutprop",
    ];
    // Without a remote signer, missing credentials simply mean a public account.
    if options.public_data_only || !options.remote_signing || DELEGATED.contains(&venue) {
        return Ok(());
    }
    if construction::bridge(venue, options).dispatch_signing
        && let Some(refs) = &options.signing
    {
        return if refs
            .primary
            .as_ref()
            .is_some_and(|id| id.validate().is_ok())
        {
            Ok(())
        } else {
            Err(invalid(
                "Rust engine private accounts require a local API key and secret; remote signing is unsupported",
            ))
        };
    }
    let present = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    if !present(&options.key) || !present(&options.secret) {
        return Err(invalid(
            "Rust engine private accounts require a local API key and secret; remote signing is unsupported",
        ));
    }
    Ok(())
}

/// Public operations one trusted setup may ask for at once.
#[cfg(feature = "dispatch-signing")]
const MAX_PUBLIC_OPERATIONS: usize = 4;

/// Trusted setup only: public values of declared refs from a provider that
/// answers inline. A host/remote provider gets `Unavailable` and nothing is
/// scheduled; its values arrive as bound account metadata instead (P4).
#[cfg(feature = "dispatch-signing")]
pub fn fetch_public(
    provider: &safe_cex_engine::signing::Provider,
    operations: Vec<safe_cex_engine::signing::Operation>,
) -> Result<Vec<Vec<u8>>, safe_cex_engine::signing::SignError> {
    use safe_cex_engine::signing::{Operation, SignError, SignRequest};
    if operations.is_empty()
        || operations.len() > MAX_PUBLIC_OPERATIONS
        || !operations.iter().all(|operation| {
            matches!(
                operation,
                Operation::PublicAddress { .. } | Operation::PublicKey { .. }
            )
        })
    {
        return Err(SignError::InvalidRequest);
    }
    let request = SignRequest {
        receipt: Default::default(),
        operations,
        context: Default::default(),
    };
    // A malformed request is refused first, whatever provider would answer.
    request.validate()?;
    if !provider.immediate() {
        return Err(SignError::Unavailable);
    }
    let mut result = provider
        .sign_ready(&request)
        .ok_or(SignError::Unavailable)??;
    request.validate_result(&result)?;
    // Widths alone admit off-curve and non-canonical values from any provider.
    if !request
        .operations
        .iter()
        .zip(&result.values)
        .all(|(operation, value)| safe_cex_crypto::signer::valid_public_value(operation, value))
    {
        return Err(SignError::InvalidResult);
    }
    Ok(std::mem::take(&mut result.values))
}

/// Trusted native/legacy setup. Consume fields once; never pass them to a venue.
#[cfg(feature = "dispatch-signing")]
pub fn prepare_local_signer(
    options: &mut EngineOptions,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    prepare_local_signer_with_policy(
        options,
        Default::default(),
        false,
        construction::ReadCredentialsPolicy::Install,
    )
}

/// Trusted legacy setup with the shared family policy applied before installation.
#[cfg(feature = "dispatch-signing")]
pub fn prepare_local_signer_for(
    venue: &str,
    options: &mut EngineOptions,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    let public_data_only = construction::signer_setup_public_data_only(options);
    #[cfg(feature = "hyperliquid")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Hyperliquid) {
        return hyperliquid_setup::native(venue, options, signer_setup_policy_for(venue, options));
    }
    #[cfg(feature = "paradex")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Paradex) {
        return paradex_setup::native(venue, options, signer_setup_policy_for(venue, options));
    }
    #[cfg(feature = "mexc")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Mexc) {
        return mexc_setup::raw(options);
    }
    #[cfg(feature = "asterdex")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Asterdex) {
        // Setup resolves the version first; the policy then reads it.
        let setup = asterdex_setup::local_setup(options, public_data_only)?;
        let policy = signer_setup_policy_for(venue, options);
        return local_signer_from_setup(setup, policy, public_data_only);
    }
    #[cfg(feature = "apexomni")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::ApexOmni) {
        let setup = apexomni_local_setup(options)?;
        let policy = signer_setup_policy_for(venue, options);
        let (provider, identities) = local_signer_from_setup(setup, policy, public_data_only)?;
        return apexomni_admit(provider, identities);
    }
    #[cfg(feature = "lighter")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Lighter) {
        payload_signing_rules(venue, options, false)?;
        return lighter_setup::native(options, signer_setup_policy_for(venue, options));
    }
    let policy = signer_setup_policy_for(venue, options);
    let read_policy = construction::read_credentials_policy(venue);
    prepare_local_signer_with_policy(options, policy, public_data_only, read_policy)
}

/// Trusted family policy plus the declarations only resolved setup facts can
/// make: AsterDEX V3 wallets are secp256k1 keys, while V1 secrets are MACs.
/// It reads only the explicit resolved version, never the material's shape.
#[cfg(feature = "dispatch-signing")]
fn signer_setup_policy_for(
    venue: &str,
    options: &EngineOptions,
) -> safe_cex_engine::signing::CredentialSetupPolicy {
    #[cfg_attr(not(feature = "asterdex"), allow(unused_mut))]
    let mut policy = construction::signer_setup_policy(venue);
    #[cfg(feature = "asterdex")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Asterdex)
        && options.asterdex.api_version == Some(safe_cex_venue_asterdex::sign::Version::V3)
    {
        for slot in [&mut policy.primary, &mut policy.secondary] {
            slot.get_or_insert_with(Default::default).signing_algorithm =
                Some(safe_cex_engine::signing::Algorithm::Secp256k1Digest);
        }
    }
    let _ = options;
    policy
}

/// ApeX has no read credentials; refused at setup rather than ignored.
#[cfg(feature = "apexomni")]
const APEXOMNI_NO_SECONDARY: &str =
    "ApeX has no read credentials; a secondary key is not supported";
/// Every ApeX private request is signed, so a private account whose key has
/// no secret is refused rather than retried as an unsignable account.
#[cfg(feature = "apexomni")]
const APEXOMNI_SECRET_REQUIRED: &str = "ApeX requires the API secret with the API key";

/// A primary with no signing material can never sign. Public data still works,
/// so a public account drops it (the venue says so); a private one is refused.
#[cfg(feature = "apexomni")]
fn apexomni_unsignable(
    identities: &mut safe_cex_engine::signing::Identities,
    public_data_only: bool,
) -> Result<bool, VenueError> {
    if !identities
        .primary
        .as_ref()
        .is_some_and(|identity| identity.signing_ref().map_or(true, |key| key.0.is_empty()))
    {
        return Ok(false);
    }
    if !public_data_only {
        return Err(invalid(APEXOMNI_SECRET_REQUIRED));
    }
    identities.primary = None;
    Ok(true)
}
/// The legacy start refusal, now at trusted setup before any order exists.
#[cfg(feature = "apexomni")]
const APEXOMNI_SEED_INVALID: &str = "Invalid OmniKey seed: expected 65-byte Ethereum signature";

/// Trusted ApeX setup: the API fields plus the OmniKey seed as the primary's
/// `zklinkSeed` role. Consumes every raw field; none reaches the venue.
#[cfg(feature = "apexomni")]
fn apexomni_local_setup(
    options: &mut EngineOptions,
) -> Result<safe_cex_crypto::signer::LocalSetup, VenueError> {
    use safe_cex_crypto::signer::{LocalExtra, LocalIdentity, LocalSetup, Secret};
    use safe_cex_engine::signing::{ExtraKey, KeyEncoding};
    if options.read_credentials.take().is_some() {
        return Err(invalid(APEXOMNI_NO_SECONDARY));
    }
    let seed = options.omni_key_seed.take().filter(|seed| !seed.is_empty());
    let any = options.key.is_some() || options.secret.is_some() || options.passphrase.is_some();
    let primary = (any || seed.is_some()).then(|| {
        let mut identity = LocalIdentity {
            key: Secret::new(options.key.take().unwrap_or_default()),
            secret: Secret::new(options.secret.take().unwrap_or_default()),
            passphrase: options.passphrase.take().map(Secret::new),
            ..Default::default()
        };
        if let Some(seed) = seed {
            identity.extra.0.insert(
                ExtraKey::ZklinkSeed,
                LocalExtra {
                    value: Secret::new(seed),
                    encoding: KeyEncoding::Hex,
                },
            );
        }
        identity
    });
    Ok(LocalSetup {
        primary,
        secondary: None,
    })
}

/// Shared by native and JSON setup: no secondary, and an installed zkLink
/// role must sign, checked by one synchronous provider probe with no I/O.
#[cfg(feature = "apexomni")]
fn apexomni_admit(
    provider: safe_cex_engine::signing::Provider,
    identities: safe_cex_engine::signing::Identities,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    use safe_cex_engine::signing::{Algorithm, ExtraKey, Operation, PayloadPart, SignRequest};
    if identities.secondary.is_some() {
        return Err(invalid(APEXOMNI_NO_SECONDARY));
    }
    if let Some(role) = identities
        .primary
        .as_ref()
        .and_then(|identity| identity.extra.get(&ExtraKey::ZklinkSeed))
    {
        let probe = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::ZkLinkContract,
                key: role.clone(),
                payload: vec![PayloadPart::Bytes { bytes: vec![0; 36] }],
            }],
        };
        if !matches!(provider.sign_ready(&probe), Some(Ok(_))) {
            return Err(invalid(APEXOMNI_SEED_INVALID));
        }
    }
    Ok((provider, identities))
}

#[cfg(feature = "dispatch-signing")]
fn prepare_local_signer_with_policy(
    options: &mut EngineOptions,
    policy: safe_cex_engine::signing::CredentialSetupPolicy,
    public_data_only: bool,
    read_policy: construction::ReadCredentialsPolicy,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, Secret};
    let primary =
        if options.key.is_some() || options.secret.is_some() || options.passphrase.is_some() {
            Some(LocalIdentity {
                key: Secret::new(options.key.take().unwrap_or_default()),
                secret: Secret::new(options.secret.take().unwrap_or_default()),
                passphrase: options.passphrase.take().map(Secret::new),
                ..Default::default()
            })
        } else {
            None
        };
    use construction::ReadCredentialsPolicy;
    // A public-only PublicRead provider holds no primary, so only Read roles select.
    let primary = primary.filter(|_| !public_read_excludes_primary(read_policy, public_data_only));
    // Trusted setup computes the result; a caller-supplied one is refused, with
    // any read material wiped first.
    if read_policy == ReadCredentialsPolicy::ValidateOnly
        && options.read_credentials_valid.is_some()
    {
        if let Some(read) = options.read_credentials.as_mut() {
            read.erase_secrets();
        }
        return Err(invalid("Unexpected read credential validation result"));
    }
    let read = options.read_credentials.take();
    let secondary = if read_policy == ReadCredentialsPolicy::ValidateOnly {
        // Validated once here; the material is wiped and never installed.
        options.read_credentials_valid = read.map(|mut read| {
            let valid = read.validate().is_ok();
            read.erase_secrets();
            valid
        });
        // Native order: the read-key refusal precedes any fallible primary decode.
        if let Some(valid) = options.read_credentials_valid {
            safe_cex_engine::ReadCredentials::status(valid).map_err(invalid)?;
        }
        None
    } else {
        read.map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        })
    };
    local_signer_from_setup(LocalSetup { primary, secondary }, policy, public_data_only)
}

#[cfg(feature = "dispatch-signing")]
fn local_signer_from_setup(
    setup: safe_cex_crypto::signer::LocalSetup,
    policy: safe_cex_engine::signing::CredentialSetupPolicy,
    public_data_only: bool,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    let (signer, identities) =
        safe_cex_crypto::signer::LocalSigner::from_setup_with_policy_and_mode(
            setup,
            policy,
            public_data_only,
        )
        .map_err(|_| invalid("Invalid local signer setup"))?;
    Ok((std::sync::Arc::new(signer), identities))
}
/// Separate trusted JS/native provider setup, with fixed parse failures.
#[cfg(feature = "dispatch-signing")]
pub fn local_signer_from_json(
    input: String,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    local_signer_from_json_with_policy(input, Default::default(), false)
}

/// Separate trusted setup with compiled family rules; not family adoption.
#[cfg(feature = "dispatch-signing")]
pub fn local_signer_from_json_for(
    venue: &str,
    input: String,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    #[cfg(feature = "mexc")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Mexc) {
        return mexc_setup::json(input, false);
    }

    // No options means no resolved setup facts, so nothing is refined.
    let (provider, identities) =
        local_signer_from_json_with_policy(input, construction::signer_setup_policy(venue), false)?;
    admit_json_setup(venue, provider, identities, false)
}

/// Venue admission of a parsed JSON setup: the family read-key rule, then
/// ApeX's OmniKey seed probe.
#[cfg(feature = "dispatch-signing")]
fn admit_json_setup(
    venue: &str,
    provider: safe_cex_engine::signing::Provider,
    identities: safe_cex_engine::signing::Identities,
    public_data_only: bool,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    read_key_identities(
        construction::read_credentials_policy(venue),
        &identities,
        public_data_only,
    )?;
    #[cfg(feature = "apexomni")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::ApexOmni) {
        return apexomni_admit(provider, identities);
    }
    Ok((provider, identities))
}

/// Separate setup that may record its resolved facts on the account options,
/// as Paradex's key form; every host's private-capsule path uses this.
#[cfg(feature = "dispatch-signing")]
pub fn local_signer_from_capsule(
    venue: &str,
    input: String,
    options: &mut EngineOptions,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    #[cfg(feature = "paradex")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Paradex) {
        let policy = signer_setup_policy_for(venue, options);
        return paradex_setup::json(venue, input, policy, options);
    }
    local_signer_from_json_for_mode(venue, input, options)
}

/// Separate setup with the parsed account options: their public/private
/// intent, before option stripping, and resolved setup facts such as AsterDEX's
/// explicit API version.
#[cfg(feature = "dispatch-signing")]
pub fn local_signer_from_json_for_mode(
    venue: &str,
    input: String,
    options: &EngineOptions,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    // Paradex records its key form on the options: `local_signer_from_capsule`.
    #[cfg(feature = "paradex")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Paradex) {
        drop(zeroize::Zeroizing::new(input));
        return Err(invalid("Invalid Paradex local signer setup"));
    }
    #[cfg(feature = "hyperliquid")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Hyperliquid) {
        return hyperliquid_setup::json(
            venue,
            input,
            signer_setup_policy_for(venue, options),
            options,
        );
    }
    #[cfg(feature = "lighter")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Lighter) {
        let mut input = zeroize::Zeroizing::new(input);
        let public = lighter_setup::setup_public_mode(options);
        payload_signing_rules_in_mode(venue, options, false, public)?;
        return lighter_setup::json(
            std::mem::take(&mut *input),
            signer_setup_policy_for(venue, options),
            options,
        );
    }

    #[cfg(feature = "mexc")]
    if construction::signer_setup_mapping(venue) == Some(construction::SetupMapping::Mexc) {
        return mexc_setup::json(input, construction::signer_setup_public_data_only(options));
    }

    let (provider, identities) = local_signer_from_json_with_policy(
        input,
        signer_setup_policy_for(venue, options),
        construction::signer_setup_public_data_only(options),
    )?;
    admit_json_setup(
        venue,
        provider,
        identities,
        construction::signer_setup_public_data_only(options),
    )
}

/// A ValidateOnly family never installs a read identity, and a public-only
/// PublicRead family never installs a primary; any other input is refused.
fn read_key_identities(
    policy: construction::ReadCredentialsPolicy,
    identities: &safe_cex_engine::signing::Identities,
    public_data_only: bool,
) -> Result<(), VenueError> {
    use construction::ReadCredentialsPolicy;
    match policy {
        ReadCredentialsPolicy::ValidateOnly if identities.secondary.is_some() => Err(invalid(
            "This venue validates read credentials without installing them",
        )),
        ReadCredentialsPolicy::PublicRead if public_data_only && identities.primary.is_some() => {
            Err(invalid(
                "Public-only read credentials cannot install a primary identity",
            ))
        }
        _ => Ok(()),
    }
}

#[cfg(feature = "dispatch-signing")]
fn local_signer_from_json_with_policy(
    input: String,
    policy: safe_cex_engine::signing::CredentialSetupPolicy,
    public_data_only: bool,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    let (signer, identities) =
        safe_cex_crypto::signer::LocalSigner::from_json_with_policy_and_mode(
            input,
            policy,
            public_data_only,
        )
        .map_err(|_| invalid("Invalid local signer setup"))?;
    Ok((std::sync::Arc::new(signer), identities))
}

/// A boolean cannot substitute for an injected provider. Validate the same
/// declaration before legacy setup consumes raw options and at the owned path.
fn payload_signing_rules(
    venue: &str,
    options: &EngineOptions,
    provider_ready: bool,
) -> Result<(), VenueError> {
    let public = construction::resolve(venue, options)
        .map_or(options.public_data_only, |mode| mode.public_data_only);
    payload_signing_rules_in_mode(venue, options, provider_ready, public)
}

fn payload_signing_rules_in_mode(
    venue: &str,
    options: &EngineOptions,
    provider_ready: bool,
    public: bool,
) -> Result<(), VenueError> {
    if !options.payload_signer || public || !supported(venue) {
        return Ok(());
    }
    // Hosted (device-key) Paradex has no Rust route in this release; its legacy
    // digest-only delegate is never constructed.
    if venue == "paradex" {
        return Err(VenueError::new(
            "Rust paradex delegated signing is unsupported in this build; configure the account's local Stark key or remove payloadSigner",
        )
        .kind(ErrorKind::Unsupported)
        .outcome(safe_cex_engine::Outcome::NotSent));
    }
    let policy = remote_signing_policy(venue);
    let identity_ready = options
        .signing
        .as_ref()
        .and_then(|refs| refs.primary.as_ref())
        .is_some_and(|identity| identity.validate().is_ok());
    if policy.enabled && policy.payload_supported && provider_ready && identity_ready {
        return Ok(());
    }
    let message = if !policy.payload_supported
        && construction::bridge(venue, options).dispatch_signing
        && policy.protected_headers.is_some()
    {
        format!("Rust {venue} has no REST-only PayloadSigner lifecycle")
    } else if policy.enabled {
        format!("Rust {venue} payloadSigner requires a configured injected signing provider")
    } else {
        format!(
            "Rust {venue} delegated signing is unsupported in this build. Remove payloadSigner and configure local signing credentials."
        )
    };
    Err(VenueError::new(message)
        .kind(ErrorKind::Unsupported)
        .outcome(safe_cex_engine::Outcome::NotSent))
}

/// Constructs `venue`'s engine from `options`, enforcing its construction rules.
pub fn build<T: Visit>(
    venue: &str,
    options: EngineOptions,
    entropy: Option<safe_cex_engine::entropy::Entropy>,
    visitor: T,
) -> Result<T::Output, VenueError> {
    payload_signing_rules(venue, &options, false)?;
    // Trusted compatibility setup consumes legacy input before venue construction.
    // All migrated execution still uses precisely one injected-provider path.
    #[cfg(feature = "dispatch-signing")]
    let (options, signer) = {
        let mut options = options;
        let signer = if construction::bridge(venue, &options).dispatch_signing
            && options.signing.is_none()
        {
            let (provider, identities) = prepare_local_signer_for(venue, &mut options)?;
            options.signing = Some(identities);
            Some(provider)
        } else {
            None
        };
        (options, signer)
    };
    #[cfg(not(feature = "dispatch-signing"))]
    let signer = None;
    build_with_signer(venue, options, entropy, signer, visitor)
}

/// The validation result travels only for a ValidateOnly family, never with
/// the material, and a false result keeps the native construction refusal.
/// The one PublicRead rule for every provider builder: a public-only provider
/// for such a family never holds, builds or probes a primary.
#[cfg(any(feature = "dispatch-signing", feature = "backpack"))]
fn public_read_excludes_primary(
    policy: construction::ReadCredentialsPolicy,
    public_data_only: bool,
) -> bool {
    public_data_only && policy == construction::ReadCredentialsPolicy::PublicRead
}

/// Host-side setup (the WASM constructor's JSON provider setup) runs this before
/// building a provider, so a false read-key result keeps native precedence over
/// any primary decode failure.
pub fn read_key_preflight(venue: &str, options: &EngineOptions) -> Result<(), VenueError> {
    match options.read_credentials_valid {
        Some(false)
            if construction::read_credentials_policy(venue)
                == construction::ReadCredentialsPolicy::ValidateOnly =>
        {
            safe_cex_engine::ReadCredentials::status(false).map_err(invalid)
        }
        _ => Ok(()),
    }
}

/// The one construction-time read-key validation, on every path. An invalid
/// key keeps the native refusal; a ValidateOnly key is replaced by its result.
/// Refused or replaced material is wiped before it is dropped.
fn validate_read_key(
    policy: construction::ReadCredentialsPolicy,
    options: &mut EngineOptions,
) -> Result<(), VenueError> {
    let Some(mut read) = options.read_credentials.take() else {
        return Ok(());
    };
    if options.read_credentials_valid.is_some() {
        read.erase_secrets();
        return Err(invalid("Unexpected read credential validation result"));
    }
    let result = read.validate();
    let validate_only = policy == construction::ReadCredentialsPolicy::ValidateOnly;
    if validate_only || result.is_err() {
        read.erase_secrets();
    } else {
        options.read_credentials = Some(read);
    }
    if validate_only {
        options.read_credentials_valid = Some(result.is_ok());
    }
    result.map_err(invalid)
}

fn read_key_options(
    policy: construction::ReadCredentialsPolicy,
    options: &EngineOptions,
) -> Result<(), VenueError> {
    let validate_only = policy == construction::ReadCredentialsPolicy::ValidateOnly;
    if validate_only && options.read_credentials.is_some() {
        return Err(invalid(
            "This venue validates read credentials without installing them",
        ));
    }
    match options.read_credentials_valid {
        Some(_) if options.read_credentials.is_some() || !validate_only => {
            return Err(invalid("Unexpected read credential validation result"));
        }
        Some(valid) => safe_cex_engine::ReadCredentials::status(valid).map_err(invalid)?,
        None => {}
    }
    if let Some(identities) = &options.signing {
        let public_data_only = construction::signer_setup_public_data_only(options);
        read_key_identities(policy, identities, public_data_only)?;
    }
    Ok(())
}

/// Venues converted for frame-fed engines (`safe_cex_engine::frame_fed`): their
/// startup honours `Cx::frame_fed` and their write paths read only the store
/// and the VENUE_STATE they export. Others are refused at construction.
pub const FRAME_FED_VENUES: &[&str] = &[
    "binance",
    "binanceinverse",
    "blofin",
    "bybit",
    "bybitinverse",
    "bybitspot",
    "bybitdemo",
    "hyperliquid",
    "hyperliquidspot",
];

pub fn frame_fed_supported(venue: &str) -> bool {
    FRAME_FED_VENUES.contains(&venue)
}

/// Generic options contain opaque references; provider owns all credentials.
pub fn build_with_signer<T: Visit>(
    venue: &str,
    mut options: EngineOptions,
    entropy: Option<safe_cex_engine::entropy::Entropy>,
    signer: Option<safe_cex_engine::signing::Provider>,
    visitor: T,
) -> Result<T::Output, VenueError> {
    let _ = &signer;
    if !supported(venue) {
        return Err(invalid(format!("unsupported venue: {venue}")));
    }
    if (options.signing.is_some() || signer.is_some())
        && !construction::bridge(venue, &options).dispatch_signing
    {
        return Err(invalid(
            "Injected signing is not supported for this venue yet",
        ));
    }
    if construction::bridge(venue, &options).dispatch_signing {
        let public_data_only = construction::signer_setup_public_data_only(&options);
        if let Some(identities) = options.signing.as_mut() {
            construction::signer_setup_policy(venue)
                .apply_to_identities(identities, public_data_only)
                .map_err(|_| invalid("Invalid local signer setup"))?;
        }
    }
    if let Some(mode) = construction::resolve(venue, &options) {
        if let Some(error) = mode.explicit_private_error {
            return Err(invalid(error));
        }
        options.public_data_only = mode.public_data_only;
        options.account_startup = !mode.public_data_only;
    }
    payload_signing_rules(venue, &options, signer.is_some())?;
    #[cfg(feature = "bybit")]
    if venue == "hyrotrader"
        && let Some(config) = options.hyrotrader.take()
    {
        options.endpoints = Some(config.resolve(options.testnet)?);
    }
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
    local_signing_rules(venue, &options)?;
    let read_policy = construction::read_credentials_policy(venue);
    validate_read_key(read_policy, &mut options)?;
    read_key_options(read_policy, &options)?;
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
    if options.frame_fed {
        if !frame_fed_supported(venue) {
            return Err(invalid(format!("frameFed is not supported for {venue}")));
        }
        if options.public_data_only {
            return Err(invalid("frameFed needs an account that can sign"));
        }
    }
    let startup = Startup {
        account: options.account_startup && !options.frame_fed,
        frame_fed: options.frame_fed,
        private_feed: remote_signing_policy(venue).private_feed(options.payload_signer),
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
                Engine::new(
                    BreakoutProp::new(BreakoutOptions {
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
                    }),
                    entropy,
                )?,
            )
        }
        #[cfg(feature = "grvt")]
        "grvt" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                Grvt::new(GrvtOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    trading_account_id: options.grvt_trading_account_id.take(),
                    signer_address: options.grvt_signer_address.take(),
                    session_proxy: options.grvt_session_proxy,
                }),
                entropy,
            )?,
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
                entropy,
            )?,
        ),
        #[cfg(feature = "polymarket")]
        "polymarket" => {
            polymarket_rules(&options)?;
            finish(
                visitor,
                policy,
                startup,
                Engine::new(
                    Polymarket::new(PolymarketOptions {
                        public_data_only: options.public_data_only,
                        key: options.key.take(),
                        polymarket: options.polymarket.take(),
                    }),
                    entropy,
                )?,
            )
        }
        #[cfg(feature = "paradex")]
        "paradex" => {
            if options.key.is_some() || options.secret.is_some() || options.passphrase.is_some() {
                return Err(invalid("Paradex signing material belongs in signer setup"));
            }
            let identities = options.signing.take().unwrap_or_default();
            if identities.secondary.is_some() || identities.primary.is_some() && signer.is_none() {
                return Err(invalid("Paradex requires its primary signing provider"));
            }
            let registered = identities.clone();
            let mut engine = Engine::new(
                Paradex::new(ParadexOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    paradex_address: options.paradex_address.take(),
                    subkey_address: options.subkey_address.take(),
                    credentials: identities.primary,
                    ethereum_key: options.paradex_ethereum_key,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "apexomni")]
        "apexomni" => {
            // Injected refs need their provider and never travel beside raw
            // material; raw native input goes through trusted setup here.
            let present = |value: &Option<String>| value.as_ref().is_some_and(|s| !s.is_empty());
            let raw = present(&options.key)
                || present(&options.secret)
                || present(&options.passphrase)
                || present(&options.omni_key_seed)
                || options.read_credentials.is_some();
            let (signer, identities) = match options.signing.take() {
                Some(identities) => {
                    if raw {
                        return Err(invalid(
                            "ApeX credentials belong in signer setup, not engine options",
                        ));
                    }
                    if signer.is_none() {
                        return Err(invalid("Engine signing provider is required"));
                    }
                    (signer, identities)
                }
                None if raw => {
                    let (provider, identities) = prepare_local_signer_for(venue, &mut options)?;
                    (Some(provider), identities)
                }
                None => (signer, Default::default()),
            };
            let mut identities = identities;
            let credentials_ignored =
                apexomni_unsignable(&mut identities, options.public_data_only)?;
            let mut engine = Engine::new(
                ApexOmni::new(ApexOmniOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    identity: identities.primary.clone(),
                    credentials_ignored,
                    tick_interval_ms: None,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(identities)?;
            finish(visitor, policy, startup, engine)
        }
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
                Engine::new(safe_cex_venue_txflow::TxFlow::new(options), entropy)?,
            )
        }
        #[cfg(feature = "kraken")]
        "kraken" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                Kraken::new(KrakenOptions {
                    read_credentials: options.read_credentials.take(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                }),
                entropy,
            )?,
        ),
        #[cfg(feature = "kraken")]
        "krakenspot" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                KrakenSpot::new(KrakenOptions {
                    // Spot private reads are POSTs and retain primary credentials.
                    read_credentials: None,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                }),
                entropy,
            )?,
        ),
        #[cfg(feature = "asterdex")]
        "asterdex" => {
            let present = |value: &Option<String>| value.as_ref().is_some_and(|s| !s.is_empty());
            if present(&options.key)
                || present(&options.secret)
                || present(&options.asterdex.private_key)
                || options.asterdex.read_key.is_some()
                || options.asterdex.read_secret.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "AsterDEX credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let installed = identities.clone();
            let mut engine = Engine::new(
                AsterDex::new(AsterOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                    read_credentials: identities.secondary,
                    api_version: options.asterdex.api_version,
                    user_address: options
                        .asterdex
                        .user_address
                        .take()
                        .filter(|text| !text.trim().is_empty())
                        .or(options.application_id.take()),
                    signer_address: options.asterdex.signer_address.take(),
                    read_signer_address: options.asterdex.read_signer_address.take(),
                    builder_address: options.asterdex.builder_address.take(),
                    builder_fee_rate_maker: options.asterdex.builder_fee_rate_maker.take(),
                    builder_fee_rate_taker: options.asterdex.builder_fee_rate_taker.take(),
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            // V3 address setup proves each slot's wallet from these identities.
            engine.set_identities(installed)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "orderly")]
        "orderly" | "tealstreet-dex" | "kodiakfi" | "mode" | "salsadex" | "what" | "woofi" => {
            orderly_rules(&options)?;
            let (provider, identities) = orderly_signer(&options)?;
            let mut engine = Engine::new(
                Orderly::new(OrderlyOptions {
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
                    identities: identities.clone(),
                }),
                entropy,
            )?;
            engine.set_signer(provider)?;
            engine.set_identities(identities)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "lighter")]
        "lighter" | "lighter-robinhood" => {
            lighter_setup::capture_lookup(&mut options);
            if options
                .lighter_l1_address
                .as_deref()
                .is_some_and(|address| {
                    address.len() != 42
                        || !address.starts_with("0x")
                        || !address[2..].bytes().all(|b| b.is_ascii_hexdigit())
                })
            {
                return Err(invalid("Invalid Lighter public L1 lookup address"));
            }
            if options.key.is_some()
                || options.secret.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Lighter credentials belong in trusted signer setup",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if identities.secondary.is_some() || (identities.primary.is_some() && signer.is_none())
            {
                return Err(invalid("Lighter requires its installed primary signer"));
            }
            let mut engine = Engine::new(
                Lighter::new(LighterOptions {
                    robinhood: venue == "lighter-robinhood",
                    remote_signer: options.lighter_remote_signer,
                    signer_provider: options.lighter_signer_provider.take(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    identity: identities.primary.clone(),
                    l1_address: options.lighter_l1_address.take(),
                    application_id: options.application_id.take(),
                    api_key_index: options.api_key_index.take(),
                    integrator_account_index: options.lighter_integrator_account_index.take(),
                    stop_slippage_pct: options.lighter_stop_slippage_pct.take().unwrap_or(1.0),
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(identities)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "bitrue")]
        "bitrue" => {
            if options.key.is_some()
                || options.secret.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Bitrue credentials belong in signer setup, not engine options",
                ));
            }
            let mut identities = options.signing.take().unwrap_or_default();
            identities.primary = identities.primary.filter(|identity| {
                !identity.credential.0.is_empty()
                    && identity.signing_ref().is_ok_and(|key| !key.0.is_empty())
            });
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if let Some(primary) = &identities.primary {
                primary.validate().map_err(invalid)?;
            }
            let registered = identities.clone();
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                Bitrue::new(BitrueOptions {
                    read_credentials: identities.secondary,
                    credentials: identities.primary,
                    public_data_only: options.public_data_only,
                    recv_window: options
                        .recv_window
                        .take()
                        .unwrap_or(20_000.)
                        .clamp(1., 60_000.) as u32,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "bitunix")]
        "bitunix" => {
            // Empty public placeholders are no credential (as WEEX).
            if [&options.key, &options.secret]
                .iter()
                .any(|value| value.as_ref().is_some_and(|s| !s.is_empty()))
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Bitunix credentials belong in signer setup, not engine options",
                ));
            }
            let mut identities = options.signing.take().unwrap_or_default();
            // Setup trims; whitespace-only material is absent, never an empty key.
            identities.primary = identities.primary.filter(|identity| {
                !identity.credential.0.is_empty()
                    && identity.signing_ref().is_ok_and(|key| !key.0.is_empty())
            });
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if let Some(primary) = &identities.primary {
                primary.validate().map_err(invalid)?;
            }
            let registered = identities.clone();
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                BitunixFutures::new(BitunixOptions {
                    read_credentials: identities.secondary,
                    credentials: identities.primary,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "backpack")]
        "backpack" => {
            // Transitional dual custody (RUST_BACKPACK_SIGNER_ADMISSION.md, option
            // B): managed trading signs through this provider, while unadopted
            // account reads and private WS auth keep the raw fields. Remove before master.
            let setup = backpack_provider(&options)?;
            let mut engine = Engine::new(
                Backpack::new(BackpackOptions {
                    identity: setup.identities.primary.clone(),
                    read_identity: setup.identities.secondary.clone(),
                    read_credentials_status: setup.read_status,
                    read_credentials: options.read_credentials.take(),
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    recv_window: options.recv_window.take(),
                }),
                entropy,
            )?;
            engine.set_signer(setup.provider)?;
            engine.set_identities(setup.identities)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "blofin")]
        "blofin" => {
            if [&options.key, &options.secret]
                .iter()
                .any(|value| value.as_ref().is_some_and(|s| !s.is_empty()))
                || options.passphrase.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "BloFin credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                Blofin::new(BlofinOptions {
                    broker_attribution: options.broker_attribution,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    identities: identities.clone(),
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(identities)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "mexc")]
        "mexc" => {
            if options.key.is_some()
                || options.secret.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid("MEXC credentials belong in signer setup"));
            }
            let identities = options.signing.take().unwrap_or_default();
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                MexcFutures::new(MexcOptions {
                    recv_window: tighter_recv_window(options.recv_window),
                    public_data_only: options.public_data_only,
                    read_credentials_valid: identities.secondary.as_ref().map(|_| true),
                    identities: identities.clone(),
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(identities)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "phemex")]
        "phemex" => {
            if options.key.as_ref().is_some_and(|s| !s.is_empty())
                || options.secret.as_ref().is_some_and(|s| !s.is_empty())
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Phemex credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            let registered = identities.clone();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                PhemexPerpetuals::new(PhemexOptions {
                    read_credentials: identities.secondary,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "binance")]
        "binance" | "binancepm" | "binancespot" | "binanceinverse" | "binanceoptions" => {
            if options.key.as_ref().is_some_and(|s| !s.is_empty())
                || options.secret.as_ref().is_some_and(|s| !s.is_empty())
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Binance credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            let registered = identities.clone();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                BinanceFutures::new(BinanceOptions {
                    signing_policy: remote_signing_policy(venue),
                    recv_window: options.recv_window,
                    read_credentials: identities.secondary,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                    portfolio_margin: venue == "binancepm",
                    variant: match venue {
                        "binancepm" => BinanceVariant::PortfolioMargin,
                        "binancespot" => BinanceVariant::Spot,
                        "binanceinverse" => BinanceVariant::CoinM,
                        "binanceoptions" => BinanceVariant::Options,
                        _ => BinanceVariant::UsdM,
                    },
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "kucoinspot")]
        "kucoinspot" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                KucoinSpot::new(KucoinOptions {
                    read_credentials: options.read_credentials.take(),
                    broker: options.extra.take().and_then(|e| e.kucoin_broker),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    passphrase: options.passphrase.take(),
                }),
                entropy,
            )?,
        ),
        #[cfg(feature = "kucoin")]
        "kucoin" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                Kucoin::new(KucoinOptions {
                    read_credentials: options.read_credentials.take(),
                    broker: options.extra.take().and_then(|e| e.kucoin_broker),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    passphrase: options.passphrase.take(),
                }),
                entropy,
            )?,
        ),
        #[cfg(feature = "bingx")]
        "bingx" => {
            if options.key.as_ref().is_some_and(|s| !s.is_empty())
                || options.secret.as_ref().is_some_and(|s| !s.is_empty())
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "BingX credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                Bingx::new(BingxOptions {
                    read_credentials: identities.secondary,
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "weex")]
        "weex" => {
            if [&options.key, &options.secret, &options.passphrase]
                .iter()
                .any(|value| value.as_ref().is_some_and(|s| !s.is_empty()))
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "WEEX credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                WeexFutures::new(WeexOptions {
                    read_credentials: identities.secondary,
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                    private_ws_headers: options.private_ws_headers,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "woo")]
        "woo" => finish(
            visitor,
            policy,
            startup,
            Engine::new(
                Woo::new(WooOptions {
                    read_credentials: options.read_credentials.take(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key.take(),
                    secret: options.secret.take(),
                    application_id: options.application_id.take(),
                }),
                entropy,
            )?,
        ),
        #[cfg(feature = "bybit")]
        "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => {
            if options.key.as_ref().is_some_and(|s| !s.is_empty())
                || options.secret.as_ref().is_some_and(|s| !s.is_empty())
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Bybit credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            let registered = identities.clone();
            if let Some(identity) = &identities.secondary {
                identity.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let options = BybitOptions {
                signing_policy: remote_signing_policy(venue),
                recv_window: tighter_recv_window(options.recv_window),
                read_credentials: identities.secondary,
                bybit_site_id: options.bybit_site_id.take(),
                remote_signing: options.remote_signing,
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
                credentials: identities.primary,
                endpoints: options.endpoints.take().map(|e| BybitEndpoints {
                    rest: e.rest,
                    public_ws: e.public_ws,
                    private_ws: e.private_ws,
                }),
            };
            options.validate().map_err(|error| invalid(&error))?;
            let mut engine = Engine::new(BybitLinear::new(options), entropy)?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "bitget")]
        "bitget" | "bitgetusdc" | "bitgetinverse" => {
            if options.key.as_ref().is_some_and(|value| !value.is_empty())
                || options
                    .secret
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                || options.passphrase.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Bitget credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if let Some(read) = &identities.secondary {
                read.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                BitgetFutures::new(BitgetOptions {
                    read_credentials: identities.secondary,
                    credentials: identities.primary,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    product: if venue == "bitgetinverse" {
                        Product::Inverse
                    } else if venue == "bitgetusdc" {
                        Product::Usdc
                    } else {
                        Product::Usdt
                    },
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "cryptocom")]
        "cryptocom" => {
            if options.key.as_ref().is_some_and(|value| !value.is_empty())
                || options
                    .secret
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
            {
                return Err(invalid(
                    "Crypto.com credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if identities.primary.is_some() && signer.is_none() {
                return Err(invalid("Engine signing provider is required"));
            }
            let registered = identities.clone();
            let mut engine = Engine::new(
                Cryptocom::new(CryptocomOptions {
                    credentials: identities.primary,
                    secondary_validation: options
                        .read_credentials_valid
                        .map(safe_cex_engine::ReadCredentials::status),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "hyperliquid")]
        "hyperliquid" | "hyperliquidspot" => {
            if options.secret.is_some()
                || options.passphrase.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "Hyperliquid signing material belongs in signer setup",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if identities.secondary.is_some() || identities.primary.is_some() && signer.is_none() {
                return Err(invalid("Hyperliquid requires its primary signing provider"));
            }
            let registered = identities.clone();
            let config = HyperliquidOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key.take(),
                credentials: identities.primary,
                application_id: options.application_id.take(),
                vault: options.vault.take(),
                builder: options.hyperliquid_builder.take().map(|b| Builder {
                    b: b.address,
                    f: b.fee_tenths_bps,
                }),
                enable_spot_markets: venue == "hyperliquid" && options.enable_spot_markets,
            };
            let mut engine = Engine::new(
                if venue == "hyperliquidspot" {
                    Hyperliquid::new_spot(config)
                } else {
                    Hyperliquid::new(config)
                },
                entropy,
            )?;
            engine.set_signer(signer)?;
            engine.set_identities(registered)?;
            finish(visitor, policy, startup, engine)
        }
        #[cfg(feature = "okx")]
        "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
            if options.key.as_ref().is_some_and(|s| !s.is_empty())
                || options.secret.as_ref().is_some_and(|s| !s.is_empty())
                || options.passphrase.is_some()
                || options.read_credentials.is_some()
            {
                return Err(invalid(
                    "OKX credentials belong in signer setup, not engine options",
                ));
            }
            let identities = options.signing.take().unwrap_or_default();
            if let Some(identity) = &identities.secondary {
                identity.validate().map_err(invalid)?;
            }
            if (identities.primary.is_some() || identities.secondary.is_some()) && signer.is_none()
            {
                return Err(invalid("Engine signing provider is required"));
            }
            let mut engine = Engine::new(
                Okx::new(OkxOptions {
                    read_credentials: identities.secondary,
                    testnet: options.testnet || venue.ends_with("demo"),
                    eea: venue.starts_with("okxeu"),
                    public_data_only: options.public_data_only,
                    credentials: identities.primary,
                }),
                entropy,
            )?;
            engine.set_signer(signer)?;
            finish(visitor, policy, startup, engine)
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
    feature = "bitunix",
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
    /// Injected (remote, delegated or hosted) refs always carry a signing ref;
    /// credential-only slots come only from trusted local setup.
    #[serde(default, deserialize_with = "EngineOptions::injected_identities")]
    pub signing: Option<safe_cex_engine::signing::Identities>,
    #[serde(default)]
    pub private_only: bool,
    /// The browser's main-thread engine, fed by a data source's frames
    /// (`safe_cex_engine::frame_fed`), for the venues in [`FRAME_FED_VENUES`].
    #[serde(default)]
    pub frame_fed: bool,
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
    /// A ValidateOnly family's read-key result, set by trusted setup in place
    /// of the material (`ReadCredentials::status`); None when no read key.
    #[serde(default)]
    pub read_credentials_valid: Option<bool>,
    #[cfg(feature = "blofin")]
    #[serde(default)]
    pub broker_attribution: bool,
    /// Regional selection suppresses cross-environment auth diagnostics; it does not change routing.
    #[cfg(feature = "bybit")]
    #[serde(default)]
    pub bybit_site_id: Option<String>,
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
    pub lighter_l1_address: Option<String>,
    #[cfg(feature = "lighter")]
    pub lighter_signer_provider: Option<String>,
    #[cfg(any(
        feature = "grvt",
        feature = "coinbase",
        feature = "paradex",
        feature = "binance",
        feature = "bitunix",
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
    /// Raw SDK caller intent; absence preserves legacy native/harness construction.
    #[serde(default)]
    pub public_mode: Option<PublicMode>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
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
    /// The caller signs remotely (a remote or payload signer) rather than with a local secret.
    #[serde(default)]
    pub remote_signing: bool,
    #[serde(default)]
    pub paradex_address: Option<String>,
    #[serde(default)]
    pub subkey_address: Option<String>,
    #[serde(default)]
    pub payload_signer: bool,
    /// Set only by trusted Paradex setup: its Stark key was derived from an
    /// Ethereum key. Never parsed, so a caller cannot claim it.
    #[cfg(feature = "paradex")]
    #[serde(skip)]
    pub(crate) paradex_ethereum_key: bool,
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
        feature = "dispatch-signing",
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
    #[serde(default)]
    pub application_id: Option<String>,
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
    #[cfg(feature = "bybit")]
    #[serde(default)]
    pub hyrotrader: Option<HyroConfig>,
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
    feature = "bitunix",
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
        self.secret.zeroize();
        #[cfg(any(
            feature = "dispatch-signing",
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
    feature = "bitunix",
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
    /// V3 read agent wallet; trusted setup defaults it to the read key.
    pub read_signer_address: Option<String>,
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
            r#"{"key":"primary-key","secret":"primary-secret","passphrase":"primary-pass","readCredentials":{"key":"read-key","secret":"read-secret","passphrase":"read-pass"}}"#,
        ).unwrap();
        assert!(std::mem::needs_drop::<EngineOptions>());
        options.erase_secrets();
        assert!(options.key.as_deref().is_none_or(str::is_empty));
        assert!(options.secret.as_deref().is_none_or(str::is_empty));
        assert!(options.passphrase.as_deref().is_none_or(str::is_empty));
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
        let missing: EngineOptions =
            serde_json::from_str(r#"{"readCredentials":{"key":"read"}}"#).unwrap();
        assert!(
            missing
                .read_credentials
                .as_ref()
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[cfg(test)]
#[path = "read_key_policy_tests.rs"]
mod read_key_policy_tests;

#[cfg(all(test, feature = "backpack"))]
#[path = "backpack_public_read_tests.rs"]
mod backpack_public_read_tests;
