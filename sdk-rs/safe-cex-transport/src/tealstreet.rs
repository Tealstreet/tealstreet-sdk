use crate::{
    provider::{
        PrivateProxy, RestAdapter, RestAdapterContext, RestAdapterRegion, RestAdapterVariant,
        RestAuthorizer, RestRequest, RestTransport, RestTransportContext, TransportError,
        TransportFuture, TransportProvider, WsConnection,
    },
    routing::{ResolvedRouting, RoutingOptions, resolve_exchange_routing},
    transport_resolution::{PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP, get_public_cors_proxy},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{collections::BTreeMap, sync::Arc};

pub trait TealstreetTransportHost {
    /// Bind the canonical accessToken facade; do not implement another token policy.
    fn get_proxy_token(&self) -> TransportFuture<'_, String>;
    fn is_node_or_bun_runtime(&self) -> bool;
    /// Source markRestProxyHeader catches sealed-config failures. Hosts must not
    /// turn diagnostic stamping into a request failure or emit token values.
    fn mark_rest_proxy_header(&self, request: &mut RestRequest, shape: &'static str);
    fn default_adapter(&self) -> Arc<dyn RestAdapter>;
    fn big_int_adapter(&self) -> Arc<dyn RestAdapter>;
    fn regional_adapter(&self, region: Option<RestAdapterRegion>) -> Arc<dyn RestAdapter>;
}
pub fn is_tealstreet_web_socket_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "tealstreet.io" || host.ends_with(".tealstreet.io"))
}
pub fn get_proxy_for_axios(
    config: &RestRequest,
    options: &ExchangeOptions,
    exchange: &ExchangeName,
) -> String {
    if config
        .base_url
        .as_ref()
        .is_some_and(|url| url.contains("binance"))
        && (options.public_data_only == Some(true)
            || config
                .headers
                .get("X-MBX-APIKEY")
                .is_none_or(String::is_empty))
    {
        get_public_cors_proxy().into()
    } else if options.tealstreet_proxy == Some(true) {
        resolve_exchange_routing(exchange, &RoutingOptions::from(options)).rest_proxy_host
    } else {
        String::new()
    }
}
pub struct TealstreetTransport {
    host: Arc<dyn TealstreetTransportHost>,
}
impl TealstreetTransport {
    pub fn new(host: Arc<dyn TealstreetTransportHost>) -> Self {
        Self { host }
    }
}
struct TealstreetAuthorizer {
    host: Arc<dyn TealstreetTransportHost>,
    exchange: ExchangeName,
}
impl RestAuthorizer for TealstreetAuthorizer {
    fn authorize<'a>(&'a self, request: &'a mut RestRequest) -> TransportFuture<'a, ()> {
        let token = self.host.get_proxy_token();
        Box::pin(async move {
            let token = token.await?;
            self.host.mark_rest_proxy_header(
                request,
                if token.is_empty() { "empty" } else { "present" },
            );
            if !token.is_empty() {
                request.headers.insert(
                    if matches!(self.exchange, ExchangeName::Grvt | ExchangeName::Mexc) {
                        "Tealstreet-Authorization"
                    } else {
                        "Authorization"
                    }
                    .into(),
                    format!("Bearer {token}"),
                );
            }
            if self.host.is_node_or_bun_runtime() {
                for (key, value) in [
                    ("Origin", "https://trade.tealstreet.io"),
                    ("Referer", "https://trade.tealstreet.io/"),
                    (
                        "User-Agent",
                        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                    ),
                    ("sec-fetch-mode", "cors"),
                    ("sec-fetch-site", "cross-site"),
                    ("sec-fetch-dest", "empty"),
                ] {
                    request.headers.insert(key.into(), value.into());
                }
            }
            Ok(())
        })
    }
}
pub struct TealstreetPrivateProxy {
    host: Arc<dyn TealstreetTransportHost>,
    url: &'static str,
    header: &'static str,
}
impl PrivateProxy for TealstreetPrivateProxy {
    fn url(&self) -> &str {
        self.url
    }
    fn resolve_headers(&self) -> TransportFuture<'_, BTreeMap<String, String>> {
        let token = self.host.get_proxy_token();
        Box::pin(async move {
            let token = token.await?;
            Ok(if token.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([(self.header.into(), format!("Bearer {token}"))])
            })
        })
    }
}
impl TransportProvider for TealstreetTransport {
    fn name(&self) -> &str {
        "tealstreet"
    }
    // `Arc` here is the TRAIT's choice, not this impl's: provider.rs declares
    // `RestTransport::authorize: Option<Arc<dyn RestAuthorizer>>` and
    // `private_proxy -> Option<Arc<dyn PrivateProxy>>`. The host these carry is
    // deliberately single-threaded (Rc-based, browser/wasm), so clippy's "replace
    // it with an Rc" cannot apply without changing the shared trait signature
    // across every transport provider. Allowed at the two sites rather than
    // workspace-wide so new Arc-of-!Send code still gets caught.
    #[allow(clippy::arc_with_non_send_sync)]
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        let excepted = matches!(context.exchange, ExchangeName::Grvt | ExchangeName::Blofin);
        let mut effective_options = context.options.clone();
        if excepted && context.options.tealstreet_tunnel == Some(true) {
            effective_options.tealstreet_proxy = Some(true);
        }
        let proxy = if context.options.tealstreet_tunnel == Some(true) && !excepted {
            String::new()
        } else {
            get_proxy_for_axios(context.config, &effective_options, context.exchange)
        };
        let proxied = !proxy.is_empty();
        let base_url = if let Some(override_url) = context
            .options
            .override_proxy_url
            .as_ref()
            .filter(|url| !url.is_empty())
        {
            Some(format!(
                "{override_url}/{}",
                context.config.base_url.as_deref().unwrap_or("undefined")
            ))
        } else if proxied {
            Some(format!(
                "{proxy}/{}",
                context.config.base_url.as_deref().unwrap_or("undefined")
            ))
        } else {
            context.config.base_url.clone()
        };
        Ok(RestTransport {
            base_url,
            proxied,
            authorize: Some(Arc::new(TealstreetAuthorizer {
                host: self.host.clone(),
                exchange: context.exchange.clone(),
            })),
        })
    }
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        Some(match context.variant {
            RestAdapterVariant::Default => self.host.default_adapter(),
            RestAdapterVariant::BigInt => self.host.big_int_adapter(),
            RestAdapterVariant::Region { region } => self.host.regional_adapter(region),
        })
    }
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
        get_proxy_for_axios(context.config, context.options, context.exchange)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        Some(get_public_cors_proxy().into())
    }
    // Same trait-mandated Arc as `rest` above.
    #[allow(clippy::arc_with_non_send_sync)]
    fn private_proxy(&self, exchange: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        Some(Arc::new(TealstreetPrivateProxy {
            host: self.host.clone(),
            url: if *exchange == ExchangeName::Grvt {
                PRIVATE_PROXY_GRVT_TINY
            } else {
                PRIVATE_PROXY_JP
            },
            header: if *exchange == ExchangeName::Grvt {
                "Tealstreet-Authorization"
            } else {
                "Authorization"
            },
        }))
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        Some(PRIVATE_PROXY_JP.into())
    }
    fn require_proxy(&self, mut options: ExchangeOptions) -> ExchangeOptions {
        options.tealstreet_proxy = Some(options.tealstreet_tunnel != Some(true));
        options
    }
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        resolve_exchange_routing(exchange, options)
    }
    fn ws_connection(&self, url: &str) -> Option<WsConnection> {
        let relay = crate::tunnel_endpoints::is_relay_url(url);
        if !relay && !is_tealstreet_web_socket_url(url) {
            return None;
        }
        let headers = BTreeMap::from([("Origin".into(),"https://preview.tealstreet.io".into()),("User-Agent".into(),"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".into())]);
        Some(if relay {
            WsConnection::Relay { headers }
        } else {
            WsConnection::Headers { headers }
        })
    }
}
