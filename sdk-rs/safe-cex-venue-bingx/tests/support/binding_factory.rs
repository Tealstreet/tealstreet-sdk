use safe_cex_transport::{
    RestAdapter, RestAdapterContext, RestAdapterVariant, RestFuture, RestRequest, RestResponse,
    RestResponseBody, RestTransport, RestTransportContext, TransportError, TransportProvider,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct AdapterCounter(AtomicUsize);
impl RestAdapter for AdapterCounter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        assert!(request.url.contains("/openApi/swap/v3/user/balance"));
        assert_eq!(
            request.timeout_ms,
            Some(20_000),
            "source factory timeout wins over API recvWindow"
        );
        let attempt = self.0.fetch_add(1, Ordering::SeqCst);
        let body = if attempt == 0 {
            json!({"code":100001,"msg":"signature verification failed"})
        } else {
            json!({"code":0,"data":[{"asset":"USDT","userId":"adapter-account","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}]})
        };
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body.to_string().into_bytes()),
            })
        })
    }
}
struct AdapterProvider(Arc<AdapterCounter>);
impl TransportProvider for AdapterProvider {
    fn name(&self) -> &str {
        "bingx-fixture"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        Ok(RestTransport {
            base_url: c.config.base_url.clone(),
            proxied: false,
            authorize: None,
        })
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        assert_eq!(c.variant, RestAdapterVariant::Default);
        Some(self.0.clone())
    }
    fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
        String::new()
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn safe_cex_transport::PrivateProxy>> {
        None
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(
        &self,
        n: &ExchangeName,
        o: &safe_cex_transport::RoutingOptions,
    ) -> safe_cex_transport::ResolvedRouting {
        safe_cex_transport::DirectTransport.ws_routing(n, o)
    }
    fn ws_connection(&self, _: &str) -> Option<safe_cex_transport::WsConnection> {
        None
    }
}
#[test]
fn factory_uses_default_adapter_for_initial_and_signature_resend() {
    let counter = Arc::new(AdapterCounter(AtomicUsize::new(0)));
    let installed = counter.clone();
    let mut opts = options();
    opts.tealstreet_tunnel = Some(true);
    let h = harness(ExchangeName::Bingx, opts, move |http| {
        let runtime = http.runtime.borrow().clone().unwrap();
        *runtime.provider.borrow_mut() = Some(Arc::new(AdapterProvider(installed)));
    });
    let account = h
        .drive(h.exchange.as_ref().get_account())
        .expect("shared adapter through legacy replay");
    assert_eq!(account.user_id, "adapter-account");
    assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    assert!(
        h.http.requests.borrow().is_empty(),
        "selected adapter owns both attempts"
    );
    done(&h);
}

#[test]
fn factory_timeout_is_independent_of_recv_window() {
    let mut opts = options();
    opts.extra = Some(serde_json::from_value(json!({"recvWindow":1234})).unwrap());
    let h = fixture_options(opts);
    h.drive(h.exchange.get_account()).unwrap();
    assert_eq!(h.http.calls(BALANCE)[0].timeout.as_f64(), Some(20_000.0), "source factory fixed timeout");
    done(&h);
}
#[test]
fn factory_preserves_axios_accept_default() {
    let h = fixture();
    h.drive(h.exchange.get_account()).unwrap();
    assert_eq!(h.http.calls(BALANCE)[0].headers["Accept"], "application/json, text/plain, */*", "source factory Accept default");
    done(&h);
}
