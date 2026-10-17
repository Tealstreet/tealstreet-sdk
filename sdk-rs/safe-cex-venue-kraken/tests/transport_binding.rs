//! The production KrakenExchange REST factory and tunnel installer against a scripted provider.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_transport::{
    DirectTransport, PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext,
    RestAdapterVariant, RestFuture, RestRequest, RestResponse, RestResponseBody, RestTransport,
    RestTransportContext, RoutingOptions, TransportError, TransportProvider, WsConnection,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{Extras, Harness, harness_with};

fn accounts() -> Value {
    json!({"result": "success", "accounts": {"flex": {
        "type": "multiCollateralMarginAccount", "portfolioValue": 1000, "availableMargin": 800,
        "totalUnrealized": 5, "currencies": {"USD": {"quantity": 1000, "value": 1000, "available": 800}}
    }}})
}

#[derive(Default)]
struct Adapter(Mutex<Vec<RestRequest>>);
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(accounts())
                        .unwrap(),
                ),
            })
        })
    }
}

struct Provider {
    adapter: Option<Arc<Adapter>>,
    routes: Mutex<Vec<ExchangeName>>,
    selections: Mutex<Vec<(ExchangeName, RestAdapterVariant)>>,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "scripted-provider"
    }
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        self.routes.lock().unwrap().push(context.exchange.clone());
        DirectTransport.rest(context)
    }
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        self.selections
            .lock()
            .unwrap()
            .push((context.exchange.clone(), context.variant));
        self.adapter
            .clone()
            .map(|adapter| adapter as Arc<dyn RestAdapter>)
    }
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(context)
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
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(exchange, options)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}

fn provider(adapter: Option<Arc<Adapter>>) -> Arc<Provider> {
    Arc::new(Provider {
        adapter,
        routes: Mutex::default(),
        selections: Mutex::default(),
    })
}

fn futures(provider: Arc<Provider>, options: Value) -> Harness {
    harness_with(
        ExchangeName::Kraken,
        serde_json::from_value(options).unwrap(),
        Extras {
            provider: Some(provider),
            ..Extras::default()
        },
        |http| http.ok("GET", "/v3/accounts", accounts()),
    )
}

fn keyed() -> Value {
    json!({"key": "fixture-key", "secret": "c2VjcmV0LWJ5dGVz"})
}

#[test]
fn real_kraken_futures_private_rest_leaves_through_the_provider_adapter() {
    let adapter = Arc::new(Adapter::default());
    let provider = provider(Some(adapter.clone()));
    let h = futures(provider.clone(), keyed());
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(result.error.as_value(), &json!(""));
    let selections = provider.selections.lock().unwrap();
    assert!(
        !selections.is_empty()
            && selections
                .iter()
                .all(|selection| selection == &(ExchangeName::Kraken, RestAdapterVariant::Default)),
        "real Kraken futures client must consult provider adapter: {selections:?}"
    );
    let tunnelled = adapter.0.lock().unwrap();
    assert!(
        tunnelled
            .iter()
            .any(|request| request.url == "/v3/accounts"),
        "real Kraken futures private read must use the provider adapter: {tunnelled:?}"
    );
    assert_eq!(tunnelled[0].timeout_ms, Some(20_000));
    assert!(
        h.http.calls("/v3/accounts").is_empty(),
        "tunnelled read must not reach the terminal host"
    );
}

#[test]
fn real_kraken_factory_consults_provider_routing_for_both_venues() {
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let provider = provider(None);
        let _h = harness_with(
            name.clone(),
            serde_json::from_value(keyed()).unwrap(),
            Extras {
                provider: Some(provider.clone()),
                ..Extras::default()
            },
            |_| {},
        );
        let routes = provider.routes.lock().unwrap();
        assert!(
            !routes.is_empty() && routes.iter().all(|route| route == &name),
            "real Kraken factory must consult provider routing: {name:?} {routes:?}"
        );
    }
}

#[test]
fn real_kraken_without_an_adapter_keeps_the_terminal_host() {
    let provider = provider(None);
    let h = futures(provider.clone(), keyed());
    h.drive(h.venue().validate_account()).unwrap();
    assert!(!provider.selections.lock().unwrap().is_empty());
    assert!(!h.http.calls("/v3/accounts").is_empty());
}

#[test]
fn real_kraken_spot_and_public_data_only_never_select_an_adapter() {
    let spot = provider(None);
    harness_with(
        ExchangeName::Krakenspot,
        serde_json::from_value(keyed()).unwrap(),
        Extras {
            provider: Some(spot.clone()),
            ..Extras::default()
        },
        |_| {},
    );
    assert!(spot.selections.lock().unwrap().is_empty());
    let public = provider(None);
    let mut options = keyed();
    options["publicDataOnly"] = json!(true);
    futures(public.clone(), options);
    assert!(public.selections.lock().unwrap().is_empty());
}
