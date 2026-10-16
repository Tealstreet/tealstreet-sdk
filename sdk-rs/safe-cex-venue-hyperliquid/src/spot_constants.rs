//! Spot subscriptions include spotClearinghouseState before orderUpdates.
pub const SINGULAR_SUBSCRIPTION_TYPES: &[&str] = &["allMids"];
pub const SYSTEM_SUBSCRIPTION_TYPES: &[&str] = &[
    "notification",
    "spotClearinghouseState",
    "orderUpdates",
    "userEvents",
    "userFills",
    "userFundings",
    "userNonFundingLedgerUpdates",
];
pub const MAINNET_API_URL: &str = "https://api.hyperliquid.xyz";
