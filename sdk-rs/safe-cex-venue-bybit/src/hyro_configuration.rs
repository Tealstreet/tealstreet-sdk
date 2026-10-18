//! Hyro account mode and bundled URL selection, independent of WS reliability.
use safe_cex_types::{ExchangeOptions, ExchangeOptionsExtraHyrotraderMode};
use serde_json::Value;

pub(crate) fn hyrotrader_mode(options: &ExchangeOptions) -> &'static str {
    if matches!(
        options
            .extra
            .as_ref()
            .and_then(|extra| extra.hyrotrader_mode.as_ref()),
        Some(ExchangeOptionsExtraHyrotraderMode::Demo)
    ) {
        "demo"
    } else {
        "production"
    }
}

pub(crate) fn get_configured_urls(
    options: &ExchangeOptions,
) -> (&'static str, &'static Value, &'static Value) {
    let simulator = (
        "simulator",
        &*crate::types::HYROTRADER_SIMULATOR_URL,
        &*crate::types::HYROTRADER_SIMULATOR_WS_URL,
    );
    if hyrotrader_mode(options) == "demo" {
        return simulator;
    }
    let production = (
        "production",
        &*crate::types::HYROTRADER_PROD_URL,
        &*crate::types::HYROTRADER_PROD_WS_URL,
    );
    let configured = |rest: &Value| {
        rest["livenet"].as_str().is_some_and(|v| !v.is_empty())
            || rest["testnet"].as_str().is_some_and(|v| !v.is_empty())
    };
    if configured(production.1) {
        return production;
    }
    if configured(simulator.1) {
        return simulator;
    }
    production
}

pub(crate) fn should_use_local_rest_order_lifecycle_fallback() -> bool {
    true
}
