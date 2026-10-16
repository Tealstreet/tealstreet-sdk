use safe_cex_transport::*;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
struct Provider {
    fixture: Value,
    calls: Rc<RefCell<Vec<Value>>>,
}
struct Proxy {
    fixture: Value,
    calls: Rc<RefCell<Vec<Value>>>,
}
impl PrivateProxy for Proxy {
    fn url(&self) -> &str {
        self.calls.borrow_mut().push(json!(["url"]));
        self.fixture["url"].as_str().unwrap()
    }
    fn resolve_headers(&self) -> TransportFuture<'_, BTreeMap<String, String>> {
        self.calls.borrow_mut().push(json!(["resolveHeaders"]));
        Box::pin(async {
            if let Some(e) = self.fixture["error"].as_str() {
                Err(TransportError::Host(e.into()))
            } else {
                Ok(serde_json::from_value(self.fixture["headers"].clone()).unwrap())
            }
        })
    }
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "fixture"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(c)
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(c)
    }
    fn proxy_prefix(&self, c: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(c)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    #[expect(clippy::arc_with_non_send_sync)]
    fn private_proxy(&self, e: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        self.calls.borrow_mut().push(json!(["privateProxy", e]));
        self.fixture["proxy"].as_bool().unwrap().then(|| {
            Arc::new(Proxy {
                fixture: self.fixture.clone(),
                calls: self.calls.clone(),
            }) as Arc<dyn PrivateProxy>
        })
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, e: &ExchangeName) -> Option<String> {
        DirectTransport.server_proxy_url(e)
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, url: &str) -> Option<WsConnection> {
        DirectTransport.ws_connection(url)
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
#[expect(clippy::arc_with_non_send_sync)]
fn approval_request_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/approval-request-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    for c in cases.as_array().unwrap() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let registry = TransportRegistry::default();
        registry.set_default_transport(Arc::new(Provider {
            fixture: c["fixture"].clone(),
            calls: calls.clone(),
        }));
        let mut future = Box::pin(
            safe_cex_venue_asterdex::builder::build_aster_approval_request(
                "https://venue/path",
                c["base"].as_str(),
                &registry,
            ),
        );
        let mut ctx = std::task::Context::from_waker(std::task::Waker::noop());
        let std::task::Poll::Ready(result) = std::future::Future::poll(future.as_mut(), &mut ctx)
        else {
            panic!("unexpected pending")
        };
        let actual = match result {
            Ok((headers, url)) => json!({"value":{"headers":headers,"url":url}}),
            Err(e) => json!({"error":e.to_string()}),
        };
        assert_eq!(actual, c["output"], "buildAsterApprovalRequest {c}");
        let expected = c["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v[0] != "getTransport")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            *calls.borrow(),
            expected,
            "buildAsterApprovalRequest trace {c}"
        );
    }
}
