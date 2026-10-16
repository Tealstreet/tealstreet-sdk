//! Source builder/address pair and subscription constants.
use safe_cex_num::{js_number_string, js_trim};
use std::borrow::Cow;

pub const SINGULAR_SUBSCRIPTION_TYPES: &[&str] = &["allMids"];
pub const SYSTEM_SUBSCRIPTION_TYPES: &[&str] = &[
    "notification",
    "orderUpdates",
    "userEvents",
    "userFills",
    "userFundings",
    "userNonFundingLedgerUpdates",
];
pub const MAINNET_API_URL: &str = "https://api.hyperliquid.xyz";
pub const TEALSTREET_BUILDER_ADDRESS_MAINNET: &str = "0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d";
pub const TEALSTREET_BUILDER_ADDRESS_TESTNET: &str = "0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba";
pub const TEALSTREET_BUILDER_FEE: f64 = 10.0;
// Preserve the source's closed allowlist and numeric own-key order. This is the
// builder/rate pair the approval page validates, not independent allowlists.
pub const HYPERLIQUID_BUILDER_TIERS: &[(f64, &str)] = &[
    (10.0, TEALSTREET_BUILDER_ADDRESS_MAINNET),
    (30.0, "0x95d699952Fd9B110ef1218369def8538CCbB094B"),
    (50.0, "0xbB7b32512e1aDadE56f2B7f092ED0A232BE80D66"),
];
pub const SPOT_ASSET_INDEX_OFFSET: u32 = 10_000;
pub const PERP_MAX_PRICE_DECIMALS: u32 = 6;
pub const SPOT_MAX_PRICE_DECIMALS: u32 = 8;

pub fn get_builder_address(testnet: bool) -> &'static str {
    if testnet {
        TEALSTREET_BUILDER_ADDRESS_TESTNET
    } else {
        TEALSTREET_BUILDER_ADDRESS_MAINNET
    }
}

pub fn hyperliquid_builder_fee_tenths_bps(address: &[u16]) -> Option<f64> {
    // No unpaired surrogate can equal an ASCII allowlist entry after JS trim
    // and lowercase. Returning None is the source result, not an ingress error.
    let address = String::from_utf16(address).ok()?;
    let wanted = js_trim(&address);
    HYPERLIQUID_BUILDER_TIERS
        .iter()
        .find_map(|(fee, builder)| builder.eq_ignore_ascii_case(wanted).then_some(*fee))
}

pub fn is_known_hyperliquid_builder(address: &[u16], fee_tenths_bps: f64) -> bool {
    // For number keys, only exact 10/30/50 can name these own properties.
    let Some((_, builder)) = HYPERLIQUID_BUILDER_TIERS
        .iter()
        .find(|(fee, _)| *fee == fee_tenths_bps)
    else {
        return false;
    };
    let Ok(address) = String::from_utf16(address) else {
        return false;
    };
    builder.eq_ignore_ascii_case(js_trim(&address))
}

#[derive(Debug, Clone, PartialEq)]
pub struct HyperliquidBuilderConfig {
    /// JavaScript string code units, including unpaired surrogates.
    pub address: Vec<u16>,
    pub fee_tenths_bps: f64,
}

/// Native input for the source's Pick<ExchangeOptions, builder | testnet>.
#[derive(Debug, Clone, Default)]
pub struct HyperliquidBuilderOptions {
    pub hyperliquid_builder: Option<HyperliquidBuilderConfig>,
    pub testnet: Option<bool>,
}

/// Source optional property reads may invoke getters and propagate their error.
pub trait HyperliquidBuilderProvider {
    type Error;
    fn hyperliquid_builder(&self)
    -> Result<Option<Cow<'_, HyperliquidBuilderConfig>>, Self::Error>;
    fn testnet(&self) -> Result<Option<bool>, Self::Error>;
}

impl HyperliquidBuilderProvider for HyperliquidBuilderOptions {
    type Error = std::convert::Infallible;
    fn hyperliquid_builder(
        &self,
    ) -> Result<Option<Cow<'_, HyperliquidBuilderConfig>>, Self::Error> {
        Ok(self.hyperliquid_builder.as_ref().map(Cow::Borrowed))
    }
    fn testnet(&self) -> Result<Option<bool>, Self::Error> {
        Ok(self.testnet)
    }
}

pub fn builder_fee_rate_from_tenths_bps(fee_tenths_bps: f64) -> String {
    // The two divisions are ordered in source. Combining /1000 changes 7's
    // signed/compared string from 0.006999999999999999% to 0.007%.
    format!("{}%", js_number_string(fee_tenths_bps / 10.0 / 100.0))
}

pub fn resolve_hyperliquid_builder<P: HyperliquidBuilderProvider + ?Sized>(
    options: Option<&P>,
) -> Result<Cow<'_, HyperliquidBuilderConfig>, P::Error> {
    if let Some(options) = options {
        if let Some(builder) = options.hyperliquid_builder()? {
            return Ok(builder);
        }
    }
    let testnet = match options {
        Some(options) => options.testnet()?.unwrap_or(false),
        None => false,
    };
    Ok(Cow::Owned(HyperliquidBuilderConfig {
        address: get_builder_address(testnet).encode_utf16().collect(),
        fee_tenths_bps: TEALSTREET_BUILDER_FEE,
    }))
}
