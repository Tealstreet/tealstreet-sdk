//! Bybit and Demo through production registry, typed API factory and local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn signature(payload: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn account(request: &support::Seen) -> Option<Value> {
    (request.path == "/v5/user/query-api").then(|| {
        json!({
            "retCode":0,"retMsg":"OK","result":{"userID":12345,"inviterID":678,"vipLevel":"No VIP"}
        })
    })
}

fn assert_signed(request: &support::Seen) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.headers["x-bapi-api-key"], "native-bybit-key");
    let payload = format!(
        "{}{}{}{}",
        request.headers["x-bapi-timestamp"],
        request.headers["x-bapi-api-key"],
        request.headers["x-bapi-recv-window"],
        request.query
    );
    assert_eq!(
        request.headers["x-bapi-sign"],
        signature(&payload),
        "native server verifies exact Bybit signed bytes"
    );
}

#[test]
fn bybit_and_demo_construct_and_sign_through_native_registry() {
    support::run_local(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert_eq!(h.exchange.base().name(), &name);
            let account =
                tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.get_account())
                    .await
                    .expect("Bybit native account read completes")
                    .unwrap();
            assert_eq!(account.user_id, "12345");
            assert_eq!(account.affiliate_id, Some(Some("678".into())));
            let seen = h.seen.borrow();
            assert_eq!(seen.len(), 1);
            assert_signed(&seen[0]);
            h.exchange.dispose().unwrap();
        }
    });
}

#[derive(Default)]
struct HostedBybit {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedBybit {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP-only witness")
        };
        let mut headers = request.config.headers;
        headers.insert("X-BAPI-API-KEY".into(), "native-bybit-key".into());
        headers.insert("X-BAPI-TIMESTAMP".into(), "1700000000000".into());
        headers.insert("X-BAPI-RECV-WINDOW".into(), "5000".into());
        headers.insert(
            "X-BAPI-SIGN".into(),
            signature(&format!(
                "1700000000000native-bybit-key5000requestId={}",
                request.config.params.as_ref().unwrap()["requestId"]
                    .as_str()
                    .unwrap()
            )),
        );
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: Some(SignedConfig {
                url: None,
                params: request.config.params,
                data: request.config.data,
                headers,
            }),
            error: None,
        });
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for HostedBybit {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn bybit_and_demo_use_hosted_signer_without_local_credentials() {
    support::run_local(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            let signer = Rc::new(HostedBybit::default());
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"","secret":""}),
                Some((signer.clone(), Some("bybit-hosted-account".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert!(h.exchange.base().options().key.is_empty());
            assert!(h.exchange.base().options().secret.is_empty());
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.get_account())
                    .await
                    .expect("secretless Bybit account read completes")
                    .unwrap();
            assert_eq!(result.user_id, "12345");
            assert_eq!(
                signer.calls.borrow().len(),
                1,
                "source options.remoteSigner reaches shared signing adapter"
            );
            let call = &signer.calls.borrow()[0];
            assert_eq!(call["exchangeName"], serde_json::to_value(name).unwrap());
            assert_eq!(call["accountName"], "bybit-hosted-account");
            assert_signed(&h.seen.borrow()[0]);
            h.exchange.dispose().unwrap();
        }
    });
}
