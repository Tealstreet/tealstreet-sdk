//! remoteSigner on the production native host: an account with an API key and no
//! secret signs every request through a hosted signer, as exchange-server does with
//! its browser relay. The local server verifies the HMAC the signer produced.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_registry::{native_host::native_venue_host_signed, rust_venues::RustVenueHost};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ws_auth::WsAuthResponse;
use safe_cex_venue_binance::private_event_value::EventValue;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use support::*;

fn hmac_hex(payload: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The browser side: holds the secret the exchange never sees, and records each request.
#[derive(Default)]
struct BrowserSigner {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for BrowserSigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let response = match request {
            RemoteSignerRequest::Http(request) => {
                let mut params = match request.config.params {
                    Some(Value::Object(map)) => map,
                    _ => Default::default(),
                };
                if let Some(Value::Object(body)) = request.config.data {
                    params.extend(body);
                }
                params.insert("timestamp".into(), json!(1_700_000_000_000u64));
                let query = safe_cex_venue_binance::signing::serialize_query(&Value::Object(
                    params.clone(),
                ));
                params.insert("signature".into(), json!(hmac_hex(&query)));
                RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    config: Some(SignedConfig {
                        url: None,
                        params: Some(Value::Object(params)),
                        data: None,
                        headers: request.config.headers,
                    }),
                    error: None,
                })
            }
            RemoteSignerRequest::WsAuth { request, .. } => {
                let to_sign = request.ws_auth.and_then(|auth| auth.to_sign).unwrap();
                RemoteSignerResponse::WsAuth(WsAuthResponse {
                    id: request.id,
                    ws_auth_signature: Some(hmac_hex(&to_sign)),
                    error: None,
                })
            }
        };
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for BrowserSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _callback: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn binance_signs_through_a_hosted_remote_signer_with_no_secret() {
    run_local(async {
        let signer = Rc::new(BrowserSigner::default());
        let Harness { exchange, seen, .. } = binance_harness_on(
            Rc::new(|_| None),
            json!({"key":"native-key","secret":""}),
            Some((signer.clone(), Some("acct-1".into()))),
        )
        .await;

        exchange.start().await.expect("start with a remote signer");
        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let ids = exchange
            .base()
            .place_order(exchange.as_ref(), order)
            .await
            .expect("placeOrder signed remotely");
        assert_eq!(ids, vec!["9001"]);

        let placed = seen
            .borrow()
            .iter()
            .rev()
            .find(|s| s.path == "/fapi/v1/order")
            .cloned()
            .expect("order reached the server");
        let signature = query_value(&placed.query, "signature").expect("signed query");
        let signed = placed.query.split("&signature=").next().unwrap();
        assert_eq!(
            signature,
            hmac_hex(signed),
            "server-verified HMAC from the remote signer over the exact bytes sent"
        );
        assert_eq!(query_value(&placed.query, "symbol"), Some("BTCUSDT"));
        assert_eq!(
            placed.headers.get("x-mbx-apikey").map(String::as_str),
            Some("native-key")
        );

        let calls = signer.calls.borrow();
        let order_call = calls
            .iter()
            .find(|c| {
                c["config"]["url"]
                    .as_str()
                    .is_some_and(|u| u.ends_with("/fapi/v1/order"))
            })
            .expect("the order went through the remote signer");
        assert_eq!(order_call["type"], "http");
        assert_eq!(order_call["exchangeName"], "binance");
        assert_eq!(order_call["accountName"], "acct-1");
        assert_eq!(order_call["config"]["method"], "POST");
        assert!(
            order_call["config"]["params"].get("signature").is_none(),
            "the exchange sent the request unsigned"
        );
        exchange.dispose().expect("dispose");
    });
}

#[test]
fn spot_websocket_auth_signs_through_the_hosted_signer() {
    run_local(async {
        let signer = Rc::new(BrowserSigner::default());
        let runtime = safe_cex_exchange::native::native_runtime();
        let host = native_venue_host_signed(
            runtime,
            Rc::new(|| {
                Ok(
                    Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap())
                        as Arc<dyn safe_cex_transport::RestAdapter>,
                )
            }),
            signer.clone(),
            Some("acct-1".into()),
        );
        let deps = host.binance().expect("binance dependencies");
        let token = deps.remote_signer_token.clone().expect("options token");
        assert_eq!(token.get("accountName").unwrap().text().unwrap(), "acct-1");
        let sign = deps.imports.remote_sign(token).expect("bound remote sign");
        let request = json!({"id":"spot-user-data","type":"ws-auth","exchangeName":"binancespot",
            "accountName":"acct-1","wsAuth":{"toSign":"apiKey=k&timestamp=1","outputFormat":"hex",
            "extraData":{"apiKey":"k","timestamp":1}}});
        let result = sign(EventValue::from_json(request)).unwrap().await.unwrap();
        assert_eq!(
            result.get("wsAuthSignature").unwrap().text().unwrap(),
            hmac_hex("apiKey=k&timestamp=1")
        );
        let calls = signer.calls.borrow();
        assert_eq!(calls[0]["type"], "ws-auth");
        assert_eq!(calls[0]["accountName"], "acct-1");
        assert_eq!(calls[0]["wsAuth"]["toSign"], "apiKey=k&timestamp=1");
    });
}
