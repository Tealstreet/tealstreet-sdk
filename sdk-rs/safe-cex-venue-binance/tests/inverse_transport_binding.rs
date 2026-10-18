//! Real inverse assembly over a recording lower transport/provider boundary.
#[path = "support/inverse_rest_runtime.rs"]
mod support;

use safe_cex_exchange::{
    js_runtime::{client::rest_http_host, promise::await_js},
    websocket::WsRuntime,
};
use safe_cex_transport::{
    DirectTransport, PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext, RestTransport,
    RestTransportContext, RoutingOptions, TransportError, TransportProvider, WsConnection,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_binance::inverse::{BinanceInverseDependencies, BinanceInverseExchange};
use serde_json::json;
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};
use support::*;

#[derive(Default)]
struct Routing {
    private_routes: Mutex<Vec<String>>,
}
impl TransportProvider for Routing {
    fn name(&self) -> &str {
        "recording-routing-provider"
    }
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        assert_eq!(context.exchange, &ExchangeName::Binanceinverse);
        self.private_routes
            .lock()
            .unwrap()
            .push(context.config.base_url.clone().unwrap());
        Ok(RestTransport {
            base_url: Some("https://private-route.invalid".into()),
            proxied: true,
            authorize: None,
        })
    }
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(context)
    }
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(context)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        DirectTransport.public_cors_proxy()
    }
    fn private_proxy(&self, exchange: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        DirectTransport.private_proxy(exchange)
    }
    fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
        DirectTransport.require_proxy(options)
    }
    fn server_proxy_url(&self, exchange: &ExchangeName) -> Option<String> {
        DirectTransport.server_proxy_url(exchange)
    }
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(exchange, options)
    }
    fn ws_connection(&self, url: &str) -> Option<WsConnection> {
        DirectTransport.ws_connection(url)
    }
}

#[test]
fn actual_public_factory_bypasses_private_provider_routing() {
    let routing = Arc::new(Routing::default());
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1_700_000_000_000.0);
    runtime.headers_capable.set(true);
    *runtime.transport_provider.borrow_mut() = Some(routing.clone());
    let http = Arc::new(Http::default());
    http.ok("GET", "/dapi/v1/exchangeInfo", json!({"symbols":[{
        "symbol":"BTCUSD_PERP","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractSize":100,"contractType":"PERPETUAL","contractStatus":"TRADING","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"10"},{"filterType":"MARKET_LOT_SIZE","minQty":"2","maxQty":"20"}]
    }]}));
    http.ok(
        "GET",
        "/dapi/v2/leverageBracket",
        json!([{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":75}]}]),
    );
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let exchange = BinanceInverseExchange::new(
        ExchangeName::Binanceinverse,
        options(),
        store.clone(),
        BinanceInverseDependencies {
            runtime: runtime.clone(),
            http: rest_http_host(runtime.clone(), http.clone()),
            external_public_data: None,
        },
    )
    .unwrap();
    let h = Harness {
        runtime,
        http,
        store,
        exchange,
    };
    let before_requests = safe_cex_exchange::http::HTTP_STATS
        .lock()
        .unwrap()
        .get_stats(h.runtime.now.get())["total"]
        .as_u64()
        .unwrap();
    let pending = h
        .exchange
        .receiver()
        .call_method("fetchMarkets", vec![])
        .unwrap();
    let markets = h
        .drive(Box::pin({
            let await_runtime: std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
                h.runtime.clone();
            async move {
                await_js(&await_runtime, pending)
                    .await
                    .map_err(|error| error.to_exchange_error())
            }
        }))
        .unwrap();
    assert_eq!(markets.get("length").unwrap().as_f64(), Some(1.0));
    assert_eq!(
        markets
            .get("0")
            .unwrap()
            .get("limits")
            .unwrap()
            .get("leverage")
            .unwrap()
            .get("max")
            .unwrap()
            .as_f64(),
        Some(75.0)
    );
    let public = h.http.calls("/dapi/v1/exchangeInfo");
    assert_eq!(public.len(), 1);
    assert_eq!(
        public[0].base_url.as_deref(),
        Some("https://dapi.binance.com"),
        "source publicNoProxy client must bypass the configured private routing provider"
    );
    assert!(public[0].skip_remote_signing);
    assert!(!public[0].headers.contains_key("X-MBX-APIKEY"));
    assert!(!public[0].url.contains("signature="));
    let private = h.http.calls("/dapi/v2/leverageBracket");
    assert_eq!(private.len(), 1);
    assert_eq!(
        private[0].base_url.as_deref(),
        Some("https://private-route.invalid")
    );
    assert_eq!(
        private[0].headers.get("X-MBX-APIKEY").map(String::as_str),
        Some("synthetic-key")
    );
    assert!(private[0].url.contains("signature="));
    assert_eq!(
        *routing.private_routes.lock().unwrap(),
        vec!["https://dapi.binance.com"; 2],
        "main and unlimited source clients route; publicNoProxy never invokes rest routing"
    );
    assert_eq!(h.runtime.transport().name(), "recording-routing-provider");
    let after_requests = safe_cex_exchange::http::HTTP_STATS
        .lock()
        .unwrap()
        .get_stats(h.runtime.now.get())["total"]
        .as_u64()
        .unwrap();
    assert_eq!(
        after_requests - before_requests,
        2,
        "source records both the public NoProxy request and the private routed request"
    );
    h.exchange
        .receiver()
        .call_method("dispose", vec![])
        .unwrap();
    assert_eq!(h.runtime.dials.borrow().len(), 0);
}
