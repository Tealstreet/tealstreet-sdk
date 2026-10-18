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
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([])).unwrap(),
        ),
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
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([])).unwrap(),
        ),
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
#[expect(
    clippy::arc_with_non_send_sync,
    reason = "TransportProvider requires Arc; this fixture stays on one thread"
)]
fn native_request_uses_the_selected_private_proxy_region_and_authorization() {
    let options = serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true,"tealstreetProxy":true,"proxyRegion":"JA","tealstreetTunnel":false})).unwrap();
    let runtime = Rc::new(support::Runtime::default());
    let wire = Arc::new(support::Wire::default());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([])).unwrap(),
        ),
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

fn secondary_options() -> ExchangeOptions {
    use base64::Engine;
    serde_json::from_value(json!({
        "key":"primary-key", "secret":base64::engine::general_purpose::STANDARD.encode([1;32]),
        "secondary_key":"reader-key", "secondary_secret":"nWGxne/9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A=",
        "tealstreetTunnel":false
    })).unwrap()
}
fn order_wire() -> Arc<support::Wire> {
    let wire = Arc::new(support::Wire::default());
    for path in ["/api/v1/orders", "/api/v1/position", "/api/v1/markets"] {
        wire.bodies.lock().unwrap().insert(
            path.into(),
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([])).unwrap(),
            ),
        );
    }
    wire
}
fn read_and_write(owner: &BackpackExchange, runtime: &support::Runtime) {
    drive(runtime, Exchange::fetch_orders(owner, None)).unwrap();
    drive(
        runtime,
        Exchange::set_leverage(owner, "BTCUSDC".into(), 5.0, None),
    )
    .unwrap();
}
#[test]
fn native_secondary_key_signs_reads_and_keeps_writes_primary() {
    use base64::Engine;
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let owner = construct(secondary_options(), runtime.clone(), wire.clone()).unwrap();
    read_and_write(owner.as_ref(), &runtime);
    let requests = wire.requests.lock().unwrap();
    let read = requests.iter().find(|r| r.url == "/api/v1/orders").unwrap();
    let write = requests.iter().find(|r| r.method == "PATCH").unwrap();
    assert_eq!(
        read.headers.get("X-API-Key").map(String::as_str),
        Some("reader-key")
    );
    assert_eq!(
        write.headers.get("X-API-Key").map(String::as_str),
        Some("primary-key")
    );
    let payload = format!(
        "instruction=orderQueryAll&timestamp={}&window={}",
        read.headers["X-Timestamp"], read.headers["X-Window"]
    );
    let signature = base64::engine::general_purpose::STANDARD
        .decode(&read.headers["X-Signature"])
        .unwrap();
    let public_key = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    let public_key: Vec<u8> = (0..public_key.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&public_key[i..i + 2], 16).unwrap())
        .collect();
    assert!(
        safe_cex_crypto::ed25519::verify(payload.as_bytes(), &public_key, &signature).unwrap(),
        "read signature must use the secondary RFC8032 seed"
    );
}
#[test]
fn native_secondary_proxy_routes_reads_separately_from_writes() {
    let mut options = secondary_options();
    options.override_proxy_url = Some("https://primary-proxy".into());
    options.secondary_proxy_url = Some("https://reader-proxy".into());
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    read_and_write(owner.as_ref(), &runtime);
    let requests = wire.requests.lock().unwrap();
    let read = requests.iter().find(|r| r.url == "/api/v1/orders").unwrap();
    let write = requests.iter().find(|r| r.method == "PATCH").unwrap();
    assert_eq!(
        read.base_url.as_deref(),
        Some("https://reader-proxy/https://api.backpack.exchange")
    );
    assert_eq!(
        write.base_url.as_deref(),
        Some("https://primary-proxy/https://api.backpack.exchange")
    );
}
#[test]
fn native_secondary_default_proxy_does_not_inherit_primary_override() {
    let mut options = secondary_options();
    options.override_proxy_url = Some("https://primary-proxy".into());
    options.secondary_proxy_url = Some(String::new());
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_orders(owner.as_ref(), None)).unwrap();
    assert_eq!(
        wire.requests.lock().unwrap()[0].base_url.as_deref(),
        Some("https://api.backpack.exchange")
    );
}
#[test]
fn native_present_empty_secondary_secret_overrides_primary_secret() {
    let mut options = secondary_options();
    options.secondary_secret = Some(String::new());
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_orders(owner.as_ref(), None)).unwrap();
    let requests = wire.requests.lock().unwrap();
    assert!(
        !requests[0].headers.contains_key("X-Signature"),
        "present empty secondary secret must disable local signing"
    );
    assert!(!requests[0].headers.contains_key("X-API-Key"));
}
#[test]
fn native_secondary_reads_do_not_consume_primary_rate_budget() {
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let owner = construct(secondary_options(), runtime.clone(), wire.clone()).unwrap();
    for index in 0..11 {
        let mut request = Exchange::fetch_markets(owner.as_ref());
        assert!(
            matches!(
                runtime.poll_checkpoint(request.as_mut()),
                Poll::Ready(Ok(_))
            ),
            "secondary read {index} must be unlimited"
        );
    }
    assert_eq!(wire.requests.lock().unwrap().len(), 11);
    for index in 0..10 {
        let mut request = owner.xhr.request(
            "PATCH",
            "/api/v1/account",
            safe_cex_num::JsJsonValue::Undefined,
            safe_cex_num::JsJsonValue::Object(vec![]),
        );
        assert!(
            matches!(
                runtime.poll_checkpoint(request.as_mut()),
                Poll::Ready(Ok(_))
            ),
            "primary write {index} retains its own budget"
        );
    }
    let mut request = owner.xhr.request(
        "PATCH",
        "/api/v1/account",
        safe_cex_num::JsJsonValue::Undefined,
        safe_cex_num::JsJsonValue::Object(vec![]),
    );
    assert!(
        runtime.poll_checkpoint(request.as_mut()).is_pending(),
        "primary writes must still stop at ten per second"
    );
    assert_eq!(wire.requests.lock().unwrap().len(), 21);
}

#[test]
fn native_secondary_options_preserve_absence_and_empty_key() {
    for secondary_key in [None, Some(""), Some("reader-key")] {
        let mut options = secondary_options();
        options.secondary_key = secondary_key.map(str::to_owned);
        options.secondary_secret = None;
        let runtime = Rc::new(support::Runtime::default());
        let wire = order_wire();
        let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
        drive(&runtime, Exchange::fetch_orders(owner.as_ref(), None)).unwrap();
        let requests = wire.requests.lock().unwrap();
        assert_eq!(
            requests[0].headers.get("X-API-Key").map(String::as_str),
            Some(if secondary_key == Some("reader-key") {
                "reader-key"
            } else {
                "primary-key"
            })
        );
        assert!(
            requests[0].headers.contains_key("X-Signature"),
            "absent secondary secret must inherit the primary secret"
        );
    }
}
#[test]
#[expect(
    clippy::arc_with_non_send_sync,
    reason = "TransportProvider fixture stays on one thread"
)]
fn native_secondary_proxy_uses_provider_public_cors_default() {
    let mut options = secondary_options();
    options.override_proxy_url = Some("https://primary-proxy".into());
    let runtime = Rc::new(support::Runtime::default());
    let wire = order_wire();
    let provider: Arc<dyn TransportProvider> =
        Arc::new(TealstreetTransport::new(Arc::new(ProxyHost(wire.clone()))));
    let expected = format!(
        "{}/https://api.backpack.exchange",
        provider.public_cors_proxy().unwrap()
    );
    *runtime.provider.borrow_mut() = Some(provider);
    let owner = construct(options, runtime.clone(), wire.clone()).unwrap();
    drive(&runtime, Exchange::fetch_orders(owner.as_ref(), None)).unwrap();
    assert_eq!(
        wire.requests.lock().unwrap()[0].base_url.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn synced_clock_reaches_native_rest_auth_and_position_mapping() {
    use base64::Engine;
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    struct ResetClock;
    impl Drop for ResetClock {
        fn drop(&mut self) {
            VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
        }
    }
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let runtime = Rc::new(support::Runtime::default());
    runtime.now.set(123.0);
    let wire = Arc::new(support::Wire::default());
    wire.bodies.lock().unwrap().insert("/api/v1/position".into(), RestResponseBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([{"symbol":"BTC_USDC_PERP","netQuantity":"2","entryPrice":"100","netExposureNotional":"200","pnlUnrealized":"3","estLiquidationPrice":"50"}])).unwrap()));
    let secret = base64::engine::general_purpose::STANDARD.encode([1; 32]);
    let owner = construct(
        serde_json::from_value(json!({"key":"clock-key","secret":secret})).unwrap(),
        runtime.clone(),
        wire.clone(),
    )
    .unwrap();
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    let positions = drive(&runtime, Exchange::fetch_positions(owner.as_ref(), None)).unwrap();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    assert_eq!(positions.len(), 1);
    let requests = wire.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|r| r.url == "/api/v1/position")
        .expect("signed position request reaches the real native transport");
    let auth_time = request.headers["X-Timestamp"].parse::<f64>().unwrap();
    assert!(!request.headers["X-Signature"].is_empty());
    let position_time = serde_json::to_value(&positions[0]).unwrap()["lastUpdate"]
        .as_f64()
        .unwrap();
    assert!(
        (before..=after).contains(&auth_time) && (before..=after).contains(&position_time),
        "synced source time {before}..={after}; REST auth={auth_time}, position lastUpdate={position_time}"
    );
}
