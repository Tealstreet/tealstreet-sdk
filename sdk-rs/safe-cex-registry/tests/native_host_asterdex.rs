//! AsterDEX constructed through the registry and production native HTTP host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct HostedAster {
    calls: RefCell<Vec<serde_json::Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedAster {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP witness")
        };
        assert!(
            !request.config.url.ends_with("/time"),
            "ASTER HOSTED bare clock bypasses signer"
        );
        if !request.config.url.ends_with("/account") {
            let response = RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                config: Some(SignedConfig {
                    url: None,
                    params: request.config.params,
                    data: request.config.data,
                    headers: request.config.headers,
                }),
                error: None,
            });
            return Box::pin(async move { Ok(response) });
        }
        assert!(
            !request
                .config
                .headers
                .keys()
                .any(|name| name.eq_ignore_ascii_case("x-mbx-apikey")),
            "source must defer private signing"
        );
        let mut headers = request.config.headers;
        headers.insert("X-MBX-APIKEY".into(), "hosted-aster-key".into());
        let params = json!({"timestamp":1700000000000u64,"recvWindow":5000,"signature":wire_signature("timestamp=1700000000000&recvWindow=5000")});
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: Some(SignedConfig {
                url: None,
                params: Some(params),
                data: request.config.data,
                headers,
            }),
            error: None,
        });
        Box::pin(async move { Ok(response) })
    }
}

#[test]
fn asterdex_hosted_public_start_keeps_bare_clock_unsigned() {
    support::run_local(async {
        let signer = Rc::new(HostedAster::default());
        let h = support::venue_harness(
            ExchangeName::Asterdex,
            Rc::new(|request| match request.path.as_str() {
                "/fapi/v1/time" => Some(json!({"serverTime":1700000000000u64})),
                "/fapi/v1/exchangeInfo" => Some(json!({"symbols":[]})),
                "/fapi/v1/ticker/24hr" => Some(json!([])),
                _ => None,
            }),
            json!({"key":"public-api-identity","secret":"","publicDataOnly":true}),
            Some((signer.clone(), Some("aster-clock-account".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.start())
            .await
            .expect("public startup completes")
            .unwrap();
        let seen = h.seen.borrow();
        let clocks: Vec<_> = seen
            .iter()
            .filter(|request| request.path.ends_with("/time"))
            .collect();
        assert_eq!(clocks.len(), 1, "ASTER HOSTED actual source clock executes");
        assert!(clocks[0].query.is_empty());
        assert!(!clocks[0].headers.contains_key("x-mbx-apikey"));
        assert!(
            signer
                .calls
                .borrow()
                .iter()
                .all(|call| !call["config"]["url"].as_str().unwrap().ends_with("/time"))
        );
        h.exchange.dispose().unwrap();
    });
}
impl RemoteSigner for HostedAster {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
fn wire_signature(query: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
    mac.update(query.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn asterdex_hosted_account_preserves_source_credentials_and_signed_wire() {
    support::run_local(async {
        for (version, path) in [("v1", "/fapi/v4/account"), ("v3", "/fapi/v3/account")] {
            let signer = Rc::new(HostedAster::default());
            let h=support::venue_harness(ExchangeName::Asterdex,Rc::new(|request| match request.path.as_str() {
                "/fapi/v1/time"|"/fapi/v3/time"=>Some(json!({"serverTime":1700000000000u64})),
                "/fapi/v4/account"|"/fapi/v3/account"=>Some(json!({"accountAlias":"hosted-native"})),_=>None
            }), json!({"key":"public-api-identity","secret":"","extra":{"asterdexApiVersion":version}}), Some((signer.clone(),Some("aster-hosted-account".into()))),safe_cex_exchange::native::native_runtime()).await;
            assert!(h.exchange.base().options().secret.is_empty());
            assert!(
                h.exchange.can_sign(),
                "ASTER HOSTED typed canSign observes signer"
            );
            assert!(!h.exchange.is_public_data_only());
            let account = h.exchange.get_account().await.unwrap();
            assert_eq!(account.user_id, "hosted-native");
            let validation = h
                .exchange
                .validate_account()
                .await
                .expect("ASTER HOSTED validateAccount uses signer without local secret");
            assert_eq!(
                validation.error,
                "".into(),
                "ASTER HOSTED validation bypasses local-secret checks"
            );
            let calls = signer.calls.borrow();
            assert_eq!(
                calls.len(),
                2,
                "ASTER HOSTED both account reads reach shared signer"
            );
            for call in calls.iter() {
                assert_eq!(call["exchangeName"], "asterdex");
                assert_eq!(call["accountName"], "aster-hosted-account");
                assert_eq!(call["config"]["url"], path);
            }
            let seen = h.seen.borrow();
            let accounts: Vec<_> = seen.iter().filter(|request| request.path == path).collect();
            assert_eq!(accounts.len(), 2);
            for account in accounts {
                assert_eq!(account.headers["x-mbx-apikey"], "hosted-aster-key");
                let (query, signature) = account.query.rsplit_once("&signature=").unwrap();
                assert_eq!(query, "timestamp=1700000000000&recvWindow=5000");
                assert_eq!(
                    signature,
                    wire_signature(query),
                    "ASTER HOSTED server verifies returned wire config"
                );
            }
            h.exchange.dispose().unwrap();
        }
    });
}

#[test]
fn asterdex_hosted_signer_retains_source_api_key_requirement() {
    support::run_local(async {
        let signer = Rc::new(HostedAster::default());
        let h = support::venue_harness(
            ExchangeName::Asterdex,
            Rc::new(|_| None),
            json!({"key":"","secret":""}),
            Some((signer.clone(), Some("aster-missing-key".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert!(!h.exchange.can_sign());
        assert!(h.exchange.is_public_data_only());
        assert_eq!(
            h.exchange.validate_account().await.unwrap_err().message,
            "Missing required credentials: API key"
        );
        assert!(signer.calls.borrow().is_empty());
        assert!(h.seen.borrow().is_empty());
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn asterdex_registry_reads_signed_account_over_native_http() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let h = support::venue_harness(
            ExchangeName::Asterdex,
            Rc::new(|request| match request.path.as_str() {
                "/fapi/v1/time" => Some(json!({"serverTime":1700000000000u64})),
                "/fapi/v4/account" => Some(json!({"accountAlias":"aster-native"})),
                _ => None,
            }),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "aster-native"
        );
        let requests = h.seen.borrow();
        let account = requests
            .iter()
            .find(|r| r.path == "/fapi/v4/account")
            .unwrap();
        assert_eq!(
            account.headers.get("x-mbx-apikey").map(String::as_str),
            Some("native-key")
        );
        let (query, signature) = account.query.rsplit_once("&signature=").unwrap();
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            signature, expected,
            "server verifies the actual signed wire query"
        );
        h.exchange.dispose().unwrap();
    });
}
