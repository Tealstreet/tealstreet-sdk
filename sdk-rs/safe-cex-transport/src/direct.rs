use crate::{
    provider::{
        PrivateProxy, RestAdapter, RestAdapterContext, RestTransport, RestTransportContext,
        TransportError, TransportProvider, WsConnection,
    },
    routing::{HowEgress, ResolvedRouting, ResolvedWs, RoutingOptions},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::sync::{Arc, Mutex};

static WARNED_WS_ROUTE: Mutex<Vec<ExchangeName>> = Mutex::new(Vec::new());

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
}

fn emit_warning(message: &str) {
    #[cfg(target_arch = "wasm32")]
    console_warn(message);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
}

#[derive(Debug, Default)]
pub struct DirectTransport;
/// The source's executable provider object; callers may use its trait methods.
pub const DIRECT_TRANSPORT: DirectTransport = DirectTransport;

impl TransportProvider for DirectTransport {
    fn name(&self) -> &str {
        "direct"
    }

    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        if context.options.tealstreet_proxy == Some(true)
            || context.options.tealstreet_tunnel == Some(true)
        {
            return Err(TransportError::DirectRoutingRequested(
                context.exchange.clone(),
            ));
        }
        let base_url = match context.options.override_proxy_url.as_deref() {
            Some(proxy) if !proxy.is_empty() => Some(format!(
                "{proxy}/{}",
                context.config.base_url.as_deref().unwrap_or("undefined")
            )),
            _ => context.config.base_url.clone(),
        };
        Ok(RestTransport {
            base_url,
            proxied: false,
            authorize: None,
        })
    }
    fn rest_adapter(&self, _: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        None
    }
    fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
        String::new()
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
        options
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        if options.tealstreet_ws_route {
            let first_warning = {
                let mut warned = WARNED_WS_ROUTE
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if warned.contains(exchange) {
                    false
                } else {
                    warned.push(exchange.clone());
                    true
                }
            };
            if first_warning {
                let name =
                    serde_json::to_string(exchange).expect("ExchangeName serializes to a string");
                // Release the dedup lock before calling a potentially reentrant host console.
                emit_warning(&format!(
                    "[{}] tealstreetWsRoute requested but the direct transport is active; connecting directly. Register a transport with setDefaultTransport() or pass ExchangeOptions.transport.",
                    name.trim_matches('"')
                ));
            }
        }
        ResolvedRouting {
            rest_proxy_host: String::new(),
            how_egress: HowEgress::Sg,
            ws_public: ResolvedWs::default(),
            ws_private: ResolvedWs::default(),
            ws_business: ResolvedWs::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{RestAdapterVariant, RestRequest};

    fn options(value: serde_json::Value) -> ExchangeOptions {
        serde_json::from_value(value).unwrap()
    }
    fn context<'a>(
        options: &'a ExchangeOptions,
        config: &'a RestRequest,
    ) -> RestTransportContext<'a> {
        RestTransportContext {
            exchange: &ExchangeName::Bybit,
            options,
            config,
        }
    }

    #[test]
    fn refuses_each_routing_flag_even_with_an_override() {
        let config = RestRequest::default();
        for flag in ["tealstreetProxy", "tealstreetTunnel"] {
            let options = options(
                serde_json::json!({"key":"k", "secret":"s", flag:true, "overrideProxyUrl":"https://override"}),
            );
            match DirectTransport.rest(context(&options, &config)) {
                Err(TransportError::DirectRoutingRequested(ExchangeName::Bybit)) => {}
                _ => panic!("{flag} must fail before any HTTP request can be sent"),
            }
        }
    }

    #[test]
    fn poisoned_warning_cache_recovers_and_routes() {
        const CHILD: &str = "SAFE_CEX_WARNING_POISON_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // An isolated test process keeps intentional poisoning out of sibling tests.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "direct::tests::poisoned_warning_cache_recovers_and_routes",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr)
                    .matches("[phemex] tealstreetWsRoute requested")
                    .count(),
                1
            );
            return;
        }
        let panic = std::panic::catch_unwind(|| {
            let _guard = WARNED_WS_ROUTE.lock().unwrap();
            panic!("intentional warning-cache poison");
        });
        assert!(panic.is_err());
        assert!(WARNED_WS_ROUTE.is_poisoned());
        let options = RoutingOptions {
            tealstreet_ws_route: true,
            ..Default::default()
        };
        for _ in 0..2 {
            let route = DirectTransport.ws_routing(&ExchangeName::Phemex, &options);
            assert_eq!(route.ws_private, ResolvedWs::default());
            assert_eq!(route.how_egress, HowEgress::Sg);
            assert!(route.rest_proxy_host.is_empty());
        }
    }

    #[test]
    fn websocket_route_warning_is_recorded_once_per_exchange() {
        let options = RoutingOptions {
            tealstreet_ws_route: true,
            ..Default::default()
        };
        DirectTransport.ws_routing(&ExchangeName::Phemex, &options);
        DirectTransport.ws_routing(&ExchangeName::Phemex, &options);
        assert_eq!(
            WARNED_WS_ROUTE
                .lock()
                .unwrap()
                .iter()
                .filter(|name| **name == ExchangeName::Phemex)
                .count(),
            1
        );
    }

    #[test]
    fn direct_rest_preserves_override_interpolation_and_absence() {
        let mut options = options(
            serde_json::json!({"key":"k", "secret":"s", "tealstreetProxy":false, "tealstreetTunnel":false}),
        );
        let mut config = RestRequest {
            base_url: Some("https://venue".into()),
            ..Default::default()
        };
        let route = DirectTransport.rest(context(&options, &config)).unwrap();
        assert_eq!(route.base_url.as_deref(), Some("https://venue"));
        assert!(!route.proxied);
        assert!(route.authorize.is_none());
        options.override_proxy_url = Some("https://override/".into());
        assert_eq!(
            DirectTransport
                .rest(context(&options, &config))
                .unwrap()
                .base_url
                .as_deref(),
            Some("https://override//https://venue")
        );
        config.base_url = None;
        assert_eq!(
            DirectTransport
                .rest(context(&options, &config))
                .unwrap()
                .base_url
                .as_deref(),
            Some("https://override//undefined")
        );
        options.override_proxy_url = Some(String::new());
        assert!(
            DirectTransport
                .rest(context(&options, &config))
                .unwrap()
                .base_url
                .is_none()
        );
    }

    #[test]
    fn direct_provider_never_supplies_intermediaries() {
        let provider: &dyn TransportProvider = &DirectTransport;
        let options = options(serde_json::json!({"key":"k", "secret":"s"}));
        let config = RestRequest::default();
        assert_eq!(provider.name(), "direct");
        assert!(
            provider
                .rest_adapter(RestAdapterContext {
                    exchange: &ExchangeName::Bybit,
                    options: &options,
                    variant: RestAdapterVariant::BigInt
                })
                .is_none()
        );
        assert!(provider.proxy_prefix(context(&options, &config)).is_empty());
        assert!(provider.public_cors_proxy().is_none());
        assert!(provider.private_proxy(&ExchangeName::Bybit).is_none());
        assert!(provider.server_proxy_url(&ExchangeName::Bybit).is_none());
        assert!(provider.ws_connection("wss://venue").is_none());
        assert_eq!(provider.require_proxy(options.clone()), options);
        for exchange in [
            ExchangeName::Bybit,
            ExchangeName::Grvt,
            ExchangeName::Binance,
        ] {
            let route = provider.ws_routing(
                &exchange,
                &RoutingOptions {
                    tealstreet_proxy: true,
                    tealstreet_ws_route: true,
                    proxy_region: Some("JA".into()),
                    bybit_site_id: Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl),
                },
            );
            assert!(route.rest_proxy_host.is_empty());
            assert_eq!(route.how_egress, HowEgress::Sg);
            assert_eq!(route.ws_public, ResolvedWs::default());
            assert_eq!(route.ws_private, ResolvedWs::default());
            assert_eq!(route.ws_business, ResolvedWs::default());
        }
    }
}
