#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[path = "support/adapter_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::ExchangeError,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestAdapterRegion, RestRequest, RestResponseBody, TransportFuture,
    TransportProvider,
    tealstreet::{TealstreetTransport, TealstreetTransportHost},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_backpack::{adapter::BackpackExchange, native_host::native_inputs};
use serde_json::json;
use std::{
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

fn construct(
    options: ExchangeOptions,
    runtime: Rc<support::Runtime>,
    wire: Arc<support::Wire>,
) -> Result<Rc<BackpackExchange>, ExchangeError> {
    let inputs = native_inputs(runtime.clone(), wire);
    BackpackExchange::new(
        BaseExchange::new(
            ExchangeName::Backpack,
            options,
            Rc::new(DefaultStore::new()),
            runtime,
            ExchangeSettings::default(),
        ),
        inputs.build_http,
        inputs.install_transport,
        inputs.runtime,
        inputs.date_parse,
    )
}
fn drive<T>(runtime: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..128 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
        runtime.now.set(runtime.now.get() + 100.0);
    }
    panic!("native request must settle")
}
#[test]
fn native_factory_keeps_proxy_flag_and_refuses_direct_transport() {
    let options = serde_json::from_value(
        json!({"key":"","secret":"","publicDataOnly":true,"tealstreetProxy":true}),
    )
    .unwrap();
    let error = construct(
        options,
        Rc::new(support::Runtime::default()),
        Arc::new(support::Wire::default()),
    )
    .err()
    .expect("proxy requests must be rejected by DirectTransport");
    assert!(
        error.message.contains("direct transport"),
        "{}",
        error.message
    );
}
#[test]
fn native_request_keeps_the_override_route_and_factory_timeout() {
    let options = serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true,"overrideProxyUrl":"https://routing-proxy"})).unwrap();
    let runtime = Rc::new(support::Runtime::default());
    let wire = Arc::new(support::Wire::default());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        RestResponseBody::Untransformed(json!([])),
    );
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_markets(owner.as_ref())).unwrap();
    let requests = wire.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].base_url.as_deref(),
        Some("https://routing-proxy/https://api.backpack.exchange")
    );
    assert_eq!(
        requests[0].timeout_ms,
        Some(20_000),
        "shared source factory timeout must reach the wire"
    );
}
struct ProxyHost(Arc<support::Wire>);
#[test]
fn native_request_keeps_the_shared_factory_timeout_without_a_proxy() {
    let options =
        serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap();
    let runtime = Rc::new(support::Runtime::default());
    let wire = Arc::new(support::Wire::default());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        RestResponseBody::Untransformed(json!([])),
    );
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_markets(owner.as_ref())).unwrap();
    assert_eq!(
        wire.requests.lock().unwrap()[0].timeout_ms,
        Some(20_000),
        "raw venue timeout must not overwrite shared factory defaults"
    );
}
impl TealstreetTransportHost for ProxyHost {
    fn get_proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok("routing-fixture-token".into()) })
    }
    fn is_node_or_bun_runtime(&self) -> bool {
        false
    }
    fn mark_rest_proxy_header(&self, _: &mut RestRequest, _: &'static str) {}
    fn default_adapter(&self) -> Arc<dyn RestAdapter> {
        self.0.clone()
    }
    fn big_int_adapter(&self) -> Arc<dyn RestAdapter> {
        self.0.clone()
    }
    fn regional_adapter(&self, _: Option<RestAdapterRegion>) -> Arc<dyn RestAdapter> {
        self.0.clone()
    }
}
#[test]
fn native_request_uses_the_selected_private_proxy_region_and_authorization() {
    let options = serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true,"tealstreetProxy":true,"proxyRegion":"JA","tealstreetTunnel":false})).unwrap();
    let runtime = Rc::new(support::Runtime::default());
    let wire = Arc::new(support::Wire::default());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        RestResponseBody::Untransformed(json!([])),
    );
    let provider: Arc<dyn TransportProvider> =
        Arc::new(TealstreetTransport::new(Arc::new(ProxyHost(wire.clone()))));
    *runtime.provider.borrow_mut() = Some(provider);
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_markets(owner.as_ref())).unwrap();
    let requests = wire.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].base_url.as_deref(),
        Some(
            "https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy/https://api.backpack.exchange"
        )
    );
    assert_eq!(
        requests[0].headers.get("Authorization").map(String::as_str),
        Some("Bearer routing-fixture-token")
    );
}
