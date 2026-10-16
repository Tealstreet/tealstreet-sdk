use crate::types::{TXFLOW_MAINNET_REST_URL, TXFLOW_TESTNET_REST_URL};
use safe_cex_exchange::http::{HttpClient, HttpClientOptions, safe_cex_axios_create};
use safe_cex_transport::{TransportError, TransportRegistry};

/// Host options carry the source `options` and `exchange` arguments of `createAPI`.
pub fn create_api(
    mut host: HttpClientOptions,
    registry: &TransportRegistry,
) -> Result<HttpClient, TransportError> {
    host.request.base_url = Some(
        if host.options.testnet == Some(true) {
            TXFLOW_TESTNET_REST_URL
        } else {
            TXFLOW_MAINNET_REST_URL
        }
        .into(),
    );
    host.request
        .headers
        .insert("Content-Type".into(), "application/json".into());
    safe_cex_axios_create(host, registry)
}
