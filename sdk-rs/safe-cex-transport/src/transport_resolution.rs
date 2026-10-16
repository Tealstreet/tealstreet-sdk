use safe_cex_types::{exchange_metadata::ExchangeMetadata, object_map::JsObjectMap};
use serde::{Deserialize, Serialize};

pub const PRIVATE_PROXY_GRVT_TINY: &str =
    "https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy";
pub const PRIVATE_PROXY_JP: &str = "https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy";
pub const PRIVATE_PROXY_ASIA: &str = "https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy";
pub const AWS_PUBLIC_CORS_PROXY: &str =
    "https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy";
pub fn get_public_cors_proxy() -> &'static str {
    AWS_PUBLIC_CORS_PROXY
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTransport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tealstreet_proxy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tealstreet_tunnel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_region: Option<String>,
}
#[derive(Deserialize)]
#[serde(remote = "ResolvedTransport", rename_all = "camelCase")]
struct ResolvedTransportWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    tealstreet_proxy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tealstreet_tunnel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_region: Option<String>,
}
impl<'de> Deserialize<'de> for ResolvedTransport {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ResolvedTransportWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

/// The source TransportMetadata Pick, without unrelated exchange fields.
#[derive(Debug, Clone, Default)]
pub struct TransportMetadata {
    pub requires_cors: bool,
    pub enforce_ws_tunnel: Option<bool>,
    pub supports_tunnel: Option<bool>,
    pub private_proxy_regions: JsObjectMap<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(remote = "TransportMetadata", rename_all = "camelCase")]
struct TransportMetadataWire {
    requires_cors: bool,
    enforce_ws_tunnel: Option<bool>,
    supports_tunnel: Option<bool>,
    private_proxy_regions: JsObjectMap<Vec<String>>,
}
impl<'de> Deserialize<'de> for TransportMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        TransportMetadataWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

impl From<&ExchangeMetadata> for TransportMetadata {
    fn from(metadata: &ExchangeMetadata) -> Self {
        Self {
            requires_cors: metadata.requires_cors,
            enforce_ws_tunnel: metadata.enforce_ws_tunnel,
            supports_tunnel: metadata.supports_tunnel,
            private_proxy_regions: metadata.private_proxy_regions.clone(),
        }
    }
}
/// Takes the source Pick's privateProxyRegions component directly.
pub fn default_proxy_region(regions: Option<&JsObjectMap<Vec<String>>>) -> String {
    regions
        .and_then(|regions| regions.keys().next())
        .filter(|key| !key.is_empty())
        .map_or_else(|| "SG".into(), |key| key.to_owned())
}
pub fn resolve_account_transport(
    account: &ResolvedTransport,
    metadata: &TransportMetadata,
) -> ResolvedTransport {
    let mut region = account.proxy_region.clone();
    if metadata.requires_cors && region.as_ref().is_none_or(String::is_empty) {
        region = Some(default_proxy_region(Some(&metadata.private_proxy_regions)));
    }
    // JS || returns its final operand, including undefined rather than false.
    let proxy = if account.tealstreet_proxy == Some(true) || metadata.requires_cors {
        Some(true)
    } else {
        metadata.enforce_ws_tunnel
    };
    let tunnel = if proxy == Some(true)
        && (metadata.supports_tunnel != Some(false) || metadata.enforce_ws_tunnel == Some(true))
    {
        Some(true)
    } else {
        account.tealstreet_tunnel
    };
    ResolvedTransport {
        tealstreet_proxy: proxy,
        tealstreet_tunnel: tunnel,
        proxy_region: region.filter(|region| !region.is_empty()),
    }
}
pub fn is_direct_transport(transport: &ResolvedTransport) -> bool {
    transport.tealstreet_proxy != Some(true) && transport.tealstreet_tunnel != Some(true)
}
