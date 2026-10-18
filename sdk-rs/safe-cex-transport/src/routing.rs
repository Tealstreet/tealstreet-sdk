use safe_cex_types::{ExchangeName, ExchangeOptions, ExchangeOptionsBybitSiteId};
use serde::{Deserialize, Serialize};

#[path = "routing_generated.rs"]
mod generated;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HowEgress {
    #[serde(rename = "jp")]
    Jp,
    #[serde(rename = "sg")]
    Sg,
}

#[derive(Deserialize)]
#[serde(remote = "HowEgress")]
enum HowEgressWire {
    #[serde(rename = "jp")]
    Jp,
    #[serde(rename = "sg")]
    Sg,
}
impl<'de> Deserialize<'de> for HowEgress {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        HowEgressWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedWs {
    pub tunnelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "ResolvedWs", rename_all = "camelCase")]
struct ResolvedWsWire {
    tunnelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    readonly: Option<String>,
}
impl<'de> Deserialize<'de> for ResolvedWs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ResolvedWsWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRouting {
    pub rest_proxy_host: String,
    pub how_egress: HowEgress,
    pub ws_public: ResolvedWs,
    pub ws_private: ResolvedWs,
    pub ws_business: ResolvedWs,
}

#[derive(Deserialize)]
#[serde(remote = "ResolvedRouting", rename_all = "camelCase")]
struct ResolvedRoutingWire {
    rest_proxy_host: String,
    how_egress: HowEgress,
    ws_public: ResolvedWs,
    ws_private: ResolvedWs,
    ws_business: ResolvedWs,
}
impl<'de> Deserialize<'de> for ResolvedRouting {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ResolvedRoutingWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

/// Site IDs use the account model's closed enum, so typos cannot select another route.
/// The typed_site_doctest_reports_e0308 gate checks this example’s actual diagnostic.
/// ```compile_fail
/// use safe_cex_transport::RoutingOptions;
/// let _ = RoutingOptions { bybit_site_id: Some(String::from("bra_btl")), ..Default::default() };
/// ```
#[derive(Debug, Default, Clone)]
pub struct RoutingOptions {
    pub tealstreet_proxy: bool,
    pub tealstreet_ws_route: bool,
    pub proxy_region: Option<String>,
    pub bybit_site_id: Option<ExchangeOptionsBybitSiteId>,
}

#[derive(Deserialize)]
#[serde(remote = "RoutingOptions", rename_all = "camelCase")]
struct RoutingOptionsWire {
    #[serde(default)]
    tealstreet_proxy: bool,
    #[serde(default)]
    tealstreet_ws_route: bool,
    proxy_region: Option<String>,
    bybit_site_id: Option<ExchangeOptionsBybitSiteId>,
}
impl<'de> Deserialize<'de> for RoutingOptions {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        RoutingOptionsWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

impl From<&ExchangeOptions> for RoutingOptions {
    fn from(options: &ExchangeOptions) -> Self {
        // Exhaustive account destructuring makes every new input require a routing decision.
        let ExchangeOptions {
            key: _,
            secret: _,
            passphrase: _,
            secondary_key: _,
            secondary_secret: _,
            secondary_passphrase: _,
            secondary_proxy_url: _,
            auth: _,
            paradex_address: _,
            subkey_address: _,
            grvt_signer_address: _,
            grvt_trading_account_id: _,
            stark_private_key: _,
            vault: _,
            application_id: _,
            api_key_index: _,
            hyperliquid_builder: _,
            testnet: _,
            enable_spot_markets: _,
            bybit_site_id,
            public_data_only: _,
            private_only: _,
            tealstreet_proxy,
            tealstreet_tunnel: _,
            tealstreet_ws_route,
            override_proxy_url: _,
            proxy_region,
            omni_key_seed: _,
            subaccount: _,
            orderly_exchange_id: _,
            tuning: _,
            extra: _,
            // Venue credentials/signature mode do not select transport routing.
            polymarket: _,
            remote_signer: _,
            transport: _,
            payload_signer: _,
            hosted_mode: _,
            external_public_data: _,
            enable_option_markets: _,
            enable_inverse_markets: _,
        } = options;
        Self {
            tealstreet_proxy: tealstreet_proxy.unwrap_or(false),
            tealstreet_ws_route: tealstreet_ws_route.unwrap_or(false),
            proxy_region: proxy_region.clone(),
            bybit_site_id: bybit_site_id.clone(),
        }
    }
}
impl From<ExchangeOptions> for RoutingOptions {
    fn from(options: ExchangeOptions) -> Self {
        Self::from(&options)
    }
}

struct StaticWs {
    tunnelled: bool,
    host: Option<&'static str>,
    fallback: Option<&'static str>,
    relay_upstream: Option<&'static str>,
    readonly: Option<&'static str>,
}
struct StaticRoute {
    rest_proxy_host: &'static str,
    ws_public: StaticWs,
    ws_private: StaticWs,
    ws_business: StaticWs,
}

// Generated from the HOW package's owned tokens; both names and base URLs use it.
pub fn is_tokyo_egress(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    generated::TOKYO_KEYS
        .iter()
        .any(|token| key.contains(token))
}

fn encode_query(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"*-._".contains(&byte) {
            out.push(char::from(byte));
        } else if byte == b' ' {
            out.push('+');
        } else {
            use std::fmt::Write;
            write!(out, "%{byte:02X}").expect("writing to a string");
        }
    }
    out
}

fn resolve_ws(ws: &StaticWs, egress: HowEgress) -> ResolvedWs {
    let endpoint = tunnel_endpoint(egress);
    ResolvedWs {
        tunnelled: ws.tunnelled,
        host: ws.host.map(str::to_owned),
        fallback: ws.fallback.map(str::to_owned),
        relay: ws
            .relay_upstream
            .map(|upstream| format!("{endpoint}?ts-relay={}", encode_query(upstream))),
        readonly: ws.readonly.map(str::to_owned),
    }
}

pub fn resolve_exchange_routing(
    exchange: &ExchangeName,
    options: &RoutingOptions,
) -> ResolvedRouting {
    let (indices, conditional, name) = generated::row(
        exchange,
        options.bybit_site_id == Some(ExchangeOptionsBybitSiteId::BraBtl),
    );
    let region = match options.proxy_region.as_deref() {
        None => 0,
        Some("JA") => 1,
        Some(_) => 2,
    };
    let index = usize::from(options.tealstreet_proxy) * 6
        + usize::from(options.tealstreet_ws_route) * 3
        + region;
    let route = &generated::ROUTES[indices[index]];
    let how_egress = if if conditional {
        options.proxy_region.as_deref() == Some("JA")
    } else {
        is_tokyo_egress(name)
    } {
        HowEgress::Jp
    } else {
        HowEgress::Sg
    };
    ResolvedRouting {
        rest_proxy_host: route.rest_proxy_host.to_owned(),
        how_egress,
        ws_public: resolve_ws(&route.ws_public, how_egress),
        ws_private: resolve_ws(&route.ws_private, how_egress),
        ws_business: resolve_ws(&route.ws_business, how_egress),
    }
}

pub(crate) fn tunnel_endpoint(egress: HowEgress) -> &'static str {
    static ENDPOINTS: std::sync::OnceLock<[String; 2]> = std::sync::OnceLock::new();
    let endpoints = ENDPOINTS.get_or_init(|| {
        #[cfg(not(target_arch = "wasm32"))]
        let staging = std::env::var("NEXT_PUBLIC_TRADETUNNEL_STAGING").ok();
        #[cfg(target_arch = "wasm32")]
        let staging = option_env!("NEXT_PUBLIC_TRADETUNNEL_STAGING").map(str::to_owned);
        [generated::TUNNEL_JP, generated::TUNNEL_SG].map(|endpoint| {
            if staging.as_deref() == Some("1") {
                format!("{}-staging", endpoint)
            } else {
                endpoint.to_owned()
            }
        })
    });
    &endpoints[match egress {
        HowEgress::Jp => 0,
        HowEgress::Sg => 1,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "explicit Node/Rustc diagnostic gate: cargo test -p safe-cex-transport -- --include-ignored"]
    fn typed_site_doctest_reports_e0308() {
        let diagnostic = std::process::Command::new("node")
            .arg("scripts/site-id-diagnostic.mjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("Node is required for the site-id diagnostic gate");
        assert!(
            diagnostic.status.success(),
            "{}",
            String::from_utf8_lossy(&diagnostic.stderr)
        );
    }

    #[test]
    #[ignore = "explicit Node/TypeScript oracle gate: yarn routing:check"]
    fn generated_routing_is_fresh_against_typescript_sources() {
        let result = std::process::Command::new("node")
            .arg("scripts/routing.mjs")
            .arg("--check")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("Node is required for the cross-language routing freshness gate");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn egress_matches_names_and_urls_case_insensitively() {
        assert!(is_tokyo_egress("HTTPS://FAPI.BINANCE.COM"));
        assert!(is_tokyo_egress("bitgetusdc"));
        assert!(!is_tokyo_egress("phemex"));
    }

    #[test]
    fn account_options_convert_without_losing_routing_fields() {
        for proxy in [None, Some(false), Some(true)] {
            for ws_route in [None, Some(false), Some(true)] {
                for region in [None, Some("JA"), Some("SG"), Some("other")] {
                    for site in [None, Some("BRA_BTL")] {
                        let mut value = serde_json::json!({"key":"k", "secret":"s"});
                        for (key, field) in [
                            ("tealstreetProxy", proxy.map(serde_json::Value::Bool)),
                            ("tealstreetWsRoute", ws_route.map(serde_json::Value::Bool)),
                            ("proxyRegion", region.map(|x| serde_json::json!(x))),
                            ("bybitSiteId", site.map(|x| serde_json::json!(x))),
                        ] {
                            if let Some(field) = field {
                                value[key] = field;
                            }
                        }
                        let account: ExchangeOptions = serde_json::from_value(value).unwrap();
                        for converted in [
                            RoutingOptions::from(&account),
                            RoutingOptions::from(account.clone()),
                        ] {
                            assert_eq!(converted.tealstreet_proxy, proxy.unwrap_or(false));
                            assert_eq!(converted.tealstreet_ws_route, ws_route.unwrap_or(false));
                            assert_eq!(converted.proxy_region.as_deref(), region);
                            assert_eq!(
                                converted.bybit_site_id,
                                site.map(|_| ExchangeOptionsBybitSiteId::BraBtl)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn site_id_rejects_typos_before_routing() {
        for invalid in ["bra_btl", "BRA-BTL", "", "unknown"] {
            let result = serde_json::from_value::<RoutingOptions>(
                serde_json::json!({"bybitSiteId": invalid}),
            );
            assert!(
                result.is_err(),
                "invalid site ID {invalid} must be rejected"
            );
        }
        let options: RoutingOptions =
            serde_json::from_value(serde_json::json!({"bybitSiteId":"BRA_BTL"})).unwrap();
        assert_eq!(
            options.bybit_site_id,
            Some(ExchangeOptionsBybitSiteId::BraBtl)
        );
        let route = resolve_exchange_routing(&ExchangeName::Bybit, &options);
        assert!(route.ws_private.tunnelled);
        assert!(
            route
                .ws_private
                .host
                .unwrap()
                .contains("/bybitbra/v5/private")
        );
    }

    #[test]
    fn non_ja_regions_use_singapore_for_pinned_families() {
        let options = RoutingOptions {
            proxy_region: Some("unexpected".into()),
            ..Default::default()
        };
        let route = resolve_exchange_routing(&ExchangeName::Bybit, &options);
        assert_eq!(route.how_egress, HowEgress::Sg);
        assert!(
            route
                .ws_private
                .relay
                .unwrap()
                .starts_with(generated::TUNNEL_SG)
        );
    }
}
