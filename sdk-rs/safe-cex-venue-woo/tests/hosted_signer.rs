//! Hosted HTTP injection must bypass venue-local HMAC, even when typed options
//! predate injection. All credentials below are synthetic, never real secrets.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::client::rest_http_host_signed,
    runtime::{Cleanup, ExchangeFuture},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportFuture,
    signing::{
        HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig, SigningConfig,
    },
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_woo::{WooDependencies, WooExchange};
use serde_json::json;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Terminal(Mutex<Vec<RestRequest>>);
impl RestAdapter for Terminal {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(RestResponse {
            status: 200, reason: None, headers: Default::default(),
            body: RestResponseBody::Encoded(br#"{"success":true,"data":{"applicationId":"hosted-id","positionMode":"ONE_WAY"}}"#.to_vec()),
        })
        })
    }
}
#[derive(Default)]
struct Signer(RefCell<Vec<SigningConfig>>);
impl safe_cex_transport::signing::RemoteSigner for Signer {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP signing expected")
        };
        assert_eq!(request.exchange_name, ExchangeName::Woo);
        assert_eq!(request.account_name.as_deref(), Some("synthetic-account"));
        assert_eq!(request.config.url, "/v3/accountinfo");
        assert_eq!(request.config.base_url, "https://api.woox.io");
        assert!(
            !request
                .config
                .headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("x-api-signature")
                    || key.eq_ignore_ascii_case("x-api-timestamp")),
            "live injected signer must bypass local WOO signing"
        );
        self.0.borrow_mut().push(request.config);
        Box::pin(async move {
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                config: Some(SignedConfig {
                    headers: [
                        (
                            "x-api-signature".into(),
                            "synthetic-hosted-signature".into(),
                        ),
                        ("x-api-timestamp".into(), "1700000000000".into()),
                    ]
                    .into(),
                    ..Default::default()
                }),
                error: None,
            }))
        })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for Signer {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
fn drive<T>(runtime: &binding_harness::Runtime, mut future: ExchangeFuture<'_, T>) -> T {
    for _ in 0..100 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result.unwrap();
        }
        runtime.pump();
    }
    panic!("hosted request did not settle");
}
fn hosted_get_account(key: &str, secret: &str) {
    let runtime = Rc::new(binding_harness::Runtime::default());
    runtime.now.set(1_700_000_000_000.0);
    let signer = Rc::new(Signer::default());
    let terminal = Arc::new(Terminal::default());
    let host = rest_http_host_signed(
        runtime.clone(),
        terminal.clone(),
        Some((signer.clone(), Some("synthetic-account".into()))),
    );
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":key,"secret":secret})).unwrap();
    assert!(
        options.remote_signer.is_none(),
        "capability arrives through shared HTTP injection"
    );
    let exchange = WooExchange::new(
        ExchangeName::Woo,
        options,
        Rc::new(DefaultStore::new()),
        WooDependencies {
            runtime: runtime.clone(),
            http: host,
            external_public_data: None,
        },
    )
    .unwrap();
    assert!(exchange.base().signers.remote_signer.is_some());
    let account = drive(&runtime, exchange.get_account());
    assert_eq!(account.user_id, "hosted-id");
    assert_eq!(signer.0.borrow().len(), 1);
    let sent = terminal.0.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].url, "/v3/accountinfo");
    assert_eq!(
        sent[0].headers.get("x-api-signature").map(String::as_str),
        Some("synthetic-hosted-signature")
    );
    assert_eq!(
        sent[0].headers.get("x-api-timestamp").map(String::as_str),
        Some("1700000000000")
    );
}
#[test]
fn secretless_hosted_account_uses_shared_signer_and_http_adapter() {
    hosted_get_account("", "");
}
#[test]
fn live_injected_signer_bypasses_hmac_with_synthetic_local_options() {
    // Keyless mode alone already skips HMAC; inert nonempty fixtures make the
    // stale typed-options check observable without any real account credential.
    hosted_get_account("synthetic-key", "synthetic-local-secret");
}
