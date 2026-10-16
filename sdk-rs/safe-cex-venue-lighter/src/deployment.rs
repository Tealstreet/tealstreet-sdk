//! Deployment helpers from lighter.types.ts; transport is resolved at call time.
use crate::types::{
    INTERVAL, LIGHTER_DEPLOYMENT_PROFILES, LighterDeploymentNetwork, LighterDeploymentProfile,
};
use safe_cex_transport::TransportRegistry;
use safe_cex_types::{ExchangeName, Timeframe};

// ts: isLighterRobinhoodExchange
pub fn is_lighter_robinhood_exchange(exchange_name: Option<&ExchangeName>) -> bool {
    matches!(exchange_name, Some(ExchangeName::LighterRobinhood))
}
// ts: resolveLighterDeploymentNetwork
pub fn resolve_lighter_deployment_network(
    exchange_name: Option<&ExchangeName>,
    testnet: Option<bool>,
) -> LighterDeploymentNetwork {
    if is_lighter_robinhood_exchange(exchange_name) {
        if testnet.unwrap_or(false) {
            LighterDeploymentNetwork::RobinhoodTestnet
        } else {
            LighterDeploymentNetwork::Robinhood
        }
    } else if testnet.unwrap_or(false) {
        LighterDeploymentNetwork::Testnet
    } else {
        LighterDeploymentNetwork::Livenet
    }
}
// Array order is pinned to the complete live protocol table. Destructuring makes
// the four-entry shape a compile-time requirement; the enum match is exhaustive.
fn profile_for_network(network: LighterDeploymentNetwork) -> LighterDeploymentProfile {
    let [
        (_, livenet),
        (_, testnet),
        (_, robinhood),
        (_, robinhood_testnet),
    ] = LIGHTER_DEPLOYMENT_PROFILES;
    match network {
        LighterDeploymentNetwork::Livenet => livenet,
        LighterDeploymentNetwork::Testnet => testnet,
        LighterDeploymentNetwork::Robinhood => robinhood,
        LighterDeploymentNetwork::RobinhoodTestnet => robinhood_testnet,
    }
}
// ts: resolveLighterDeploymentProfile
pub fn resolve_lighter_deployment_profile(
    exchange_name: Option<&ExchangeName>,
    testnet: Option<bool>,
) -> LighterDeploymentProfile {
    let network = resolve_lighter_deployment_network(exchange_name, testnet);
    profile_for_network(network)
}
// ts: getBaseUrl
pub fn get_base_url(network: LighterDeploymentNetwork, registry: &TransportRegistry) -> String {
    let profile = profile_for_network(network);
    if profile.uses_rest_proxy {
        registry.with_public_cors_proxy(&profile.api_base_url, None)
    } else {
        profile.api_base_url.into_owned()
    }
}
// ts: getLighterApiBaseUrl
pub fn get_lighter_api_base_url(
    exchange_name: Option<&ExchangeName>,
    testnet: Option<bool>,
    registry: &TransportRegistry,
) -> String {
    get_base_url(
        resolve_lighter_deployment_network(exchange_name, testnet),
        registry,
    )
}
// ts: getLighterWsUrl
pub fn get_lighter_ws_url(exchange_name: Option<&ExchangeName>, testnet: Option<bool>) -> String {
    resolve_lighter_deployment_profile(exchange_name, testnet)
        .ws_url
        .into_owned()
}
// ts: resolveLighterInterval
pub fn resolve_lighter_interval(interval: &Timeframe) -> Result<&'static str, String> {
    let interval = serde_json::to_value(interval).map_err(|error| error.to_string())?;
    let interval = interval
        .as_str()
        .ok_or_else(|| "Lighter interval must serialize to a string".to_owned())?;
    INTERVAL.iter().find(|(key, _)| *key == interval).map(|(_, value)| *value).ok_or_else(|| format!("Lighter does not support raw {interval} candles. Supported resolutions: 1m, 5m, 15m, 30m, 1h, 4h, 12h, 1d."))
}
