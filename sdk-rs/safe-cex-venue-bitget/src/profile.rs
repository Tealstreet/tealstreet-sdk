//! Pure account-profile getters; receiver testnet state is an explicit input.

// ts: BitgetExchange.apiProductType
pub fn api_product_type(testnet: bool) -> &'static str {
    if testnet {
        "susdt-futures"
    } else {
        "usdt-futures"
    }
}
// ts: BitgetExchange.apiMarginCoin
pub fn api_margin_coin(testnet: bool) -> &'static str {
    if testnet { "SUSDT" } else { "USDT" }
}
// ts: BitgetUSDCExchange.apiProductType
// ts-bug: BitgetUSDCExchange.apiProductType declares USDT literals through an any cast; preserve its actual variant value.
pub fn usdc_api_product_type(testnet: bool) -> &'static str {
    if testnet {
        "susdc-futures"
    } else {
        "usdc-futures"
    }
}
// ts: BitgetUSDCExchange.apiMarginCoin
// ts-bug: BitgetUSDCExchange.apiMarginCoin declares USDT literals through an any cast; preserve its actual variant value.
pub fn usdc_api_margin_coin(testnet: bool) -> &'static str {
    if testnet { "SUSDC" } else { "USDC" }
}
// ts: BitgetInverseExchange.apiProductType
// ts-bug: BitgetInverseExchange.apiProductType declares USDT literals through an any cast; preserve its actual variant value.
pub fn inverse_api_product_type(testnet: bool) -> &'static str {
    if testnet {
        "scoin-futures"
    } else {
        "coin-futures"
    }
}
// ts: BitgetInverseExchange.apiMarginCoin
// ts-bug: BitgetInverseExchange.apiMarginCoin declares USDT literals through an any cast; preserve its actual variant value.
pub fn inverse_api_margin_coin() -> &'static str {
    "BTC"
}

// ts: BitgetExchange.apiUtaCategory
pub fn api_uta_category(api_product_type: &str) -> String {
    // ts-bug: BitgetExchange.apiUtaCategory casts testnet S-prefixed strings
    // to BITGET_UTA_CATEGORY, whose declaration excludes all those strings.
    // Keep the actual output instead of coercing it to the closed wire enum.
    api_product_type.to_uppercase()
}

// ts: BitgetExchange.usesUtaRouting
pub fn uses_uta_routing(is_uta: bool, supports_uta_routing: bool, testnet: bool) -> bool {
    is_uta && supports_uta_routing && !testnet
}

// ts: BitgetExchange.suppressBitgetValidationErrorEmission
pub fn suppress_bitget_validation_error_emission(suppressed: bool) -> bool {
    suppressed
}
