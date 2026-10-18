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
use safe_cex_venue_binance::private_event_value::{EventOps, EventValue};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
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
                if let Some(Value::Object(body)) =
                    request.config.data.map(|value| value.into_value().unwrap())
                {
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

/// A browser relay returns the signed configuration; this host has no secret.
#[derive(Default)]
struct AccountRelay {
    calls: RefCell<Vec<Value>>,
    reject_ws: Cell<bool>,
    replace_params: Cell<bool>,
}
impl safe_cex_transport::signing::RemoteSigner for AccountRelay {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let response = match request {
            RemoteSignerRequest::Http(request) => {
                let mut headers = request.config.headers;
                headers.insert(
                    "x-relay-signature".into(),
                    hmac_hex(&format!("{} {}", request.config.method, request.config.url)),
                );
                RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    config: Some(SignedConfig {
                        url: None,
                        params: if self.replace_params.get() {
                            Some(json!({"x":"signed"}))
                        } else {
                            request.config.params
                        },
                        data: request.config.data,
                        headers,
                    }),
                    error: None,
                })
            }
            RemoteSignerRequest::WsAuth { request, .. } => {
                let input = request.ws_auth.and_then(|auth| auth.to_sign).unwrap();
                RemoteSignerResponse::WsAuth(WsAuthResponse {
                    id: request.id,
                    ws_auth_signature: (!self.reject_ws.get()).then(|| hmac_hex(&input)),
                    error: self
                        .reject_ws
                        .get()
                        .then(|| "hosted signer unavailable".into()),
                })
            }
        };
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for AccountRelay {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn tier_one_native_venues_validate_through_the_hosted_account_without_a_secret() {
    use safe_cex_types::ExchangeName;
    run_local(async {
        for name in [
            ExchangeName::Okx,
            ExchangeName::Okxdemo,
            ExchangeName::Okxeu,
            ExchangeName::Okxeudemo,
            ExchangeName::Blofin,
            ExchangeName::Bitunix,
        ] {
            let signer = Rc::new(AccountRelay::default());
            let h = venue_harness(name.clone(), Rc::new(|request| {
                assert_eq!(request.headers.get("x-relay-signature"), Some(&hmac_hex(&format!("{} {}", request.method, request.path))), "server verifies the hosted relay signature");
                Some(match request.path.as_str() {
                    "/api/v1/asset/balances" => json!({"code":"0","data":[{"available":"61","frozen":"7"}]}),
                    "/api/v1/futures/account" => json!({"code":0,"data":[{"marginCoin":"USDT","available":"61","margin":"7","crossUnrealizedPNL":"0","isolationUnrealizedPNL":"0"}]}),
                    _ => json!({"code":"0","data":[]}),
                })
            }), json!({"key":"hosted-key","secret":"","passphrase":"hosted-passphrase"}), Some((signer.clone(), Some("hosted-account".into()))), safe_cex_exchange::native::native_runtime()).await;
            assert!(
                h.exchange.base().signers.remote_signer.is_some(),
                "native core retains hosted signer for {name:?}"
            );
            assert!(h.exchange.base().options().secret.is_empty());
            let result = h
                .exchange
                .validate_account()
                .await
                .expect("hosted validation completes");
            assert_eq!(
                result.error.as_value(),
                &json!(""),
                "hosted account validates for {name:?}"
            );
            assert!(
                !h.seen.borrow().is_empty(),
                "validation reaches real local HTTP"
            );
            let calls = signer.calls.borrow();
            assert!(!calls.is_empty(), "hosted signer called for {name:?}");
            for call in calls.iter() {
                assert_eq!(call["exchangeName"], serde_json::to_value(&name).unwrap());
                assert_eq!(call["accountName"], "hosted-account");
            }
            h.exchange.dispose().unwrap();
        }
    });
}

#[test]
fn tier_one_private_sockets_authenticate_through_the_hosted_account() {
    use safe_cex_types::ExchangeName;
    run_local(async {
        for name in [
            ExchangeName::Okx,
            ExchangeName::Blofin,
            ExchangeName::Bitunix,
        ] {
            let signer = Rc::new(AccountRelay::default());
            let h = venue_harness(
                name.clone(),
                Rc::new(|request| Some(match request.path.as_str() {
                    "/api/v5/account/config" => json!({"code":"0","data":[{"posMode":"net_mode"}]}),
                    "/api/v1/futures/account" => json!({"code":0,"data":{"marginCoin":"USDT","available":"61","margin":"7","positionMode":"ONE_WAY"}}),
                    path if path.starts_with("/api/v1/futures/") => json!({"code":0,"data":[]}),
                    _ => json!({"code":"0","data":[]}),
                })),
                json!({"key":"hosted-key","secret":"","passphrase":"hosted-passphrase"}),
                Some((signer.clone(), Some("hosted-account".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            let exchange = h.exchange.clone();
            let startup = tokio::task::spawn_local(async move { exchange.start().await });
            eventually("hosted login reaches the private socket", || {
                h.received
                    .borrow()
                    .iter()
                    .any(|frame| frame.contains("\"op\":\"login\""))
            })
            .await;
            let calls = signer.calls.borrow();
            let call = calls
                .iter()
                .find(|call| call["type"] == "ws-auth")
                .expect("private auth uses the hosted signer");
            assert_eq!(call["exchangeName"], serde_json::to_value(&name).unwrap());
            assert_eq!(call["accountName"], "hosted-account");
            let signature = hmac_hex(call["wsAuth"]["toSign"].as_str().unwrap());
            assert!(
                h.received
                    .borrow()
                    .iter()
                    .any(|frame| frame.contains(&signature)),
                "private login carries the returned signature for {name:?}"
            );
            h.exchange.dispose().unwrap();
            startup.abort();
        }
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
            safe_cex_registry::native_host::hyperliquid_rate_limiter(),
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

#[test]
fn rejected_hosted_private_auth_sends_no_login_or_subscriptions() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    use safe_cex_types::ExchangeName;
    run_local(async {
        let signer = Rc::new(AccountRelay {
            reject_ws: Cell::new(true),
            ..Default::default()
        });
        let h = venue_harness(
            ExchangeName::Bitunix,
            Rc::new(|_| None),
            json!({"key":"hosted-key","secret":""}),
            Some((signer.clone(), Some("hosted-account".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange
            .base()
            .get_socket(SocketSlot::Private)
            .expect("Bitunix private socket")
            .connect_and_subscribe(false)
            .unwrap();
        eventually("hosted signer rejects private auth", || {
            signer
                .calls
                .borrow()
                .iter()
                .any(|call| call["type"] == "ws-auth")
        })
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert!(
            !h.received
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"op\":\"login\"")
                    || frame.contains("\"op\":\"subscribe\"")),
            "rejected hosted auth sends no login or subscription"
        );
        h.exchange.dispose().unwrap();
    });
}

/// Custom query rendering consumes the returned signed params, while the retry
/// config retains those params and the source URL instead of wire-only encoding.
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn hosted_signing_precedes_query_serialization_and_retains_signed_config() {
    use safe_cex_exchange::js_runtime::{
        client,
        value::{Js, to_json},
    };
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    };
    struct Wire(Rc<RefCell<Vec<RestRequest>>>);
    impl RestAdapter for Wire {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.0.borrow_mut().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Encoded(b"{}".to_vec()),
                })
            })
        }
    }
    run_local(async {
        let signer = Rc::new(AccountRelay {
            replace_params: Cell::new(true),
            ..Default::default()
        });
        let seen = Rc::new(RefCell::new(Vec::new()));
        let host = client::rest_http_host_signed(
            safe_cex_exchange::native::native_runtime(),
            Arc::new(Wire(seen.clone())),
            Some((signer.clone(), Some("query-account".into()))),
        );
        host.configure_exchange(safe_cex_types::ExchangeName::Okx)
            .unwrap();
        let response = host
            .send(Js::object(vec![
                ("method", Js::str("get")),
                ("url", Js::str("/private")),
                ("params", Js::object(vec![("x", Js::str("unsigned"))])),
                (
                    "paramsSerializer",
                    Js::function("serialize", |_, args| {
                        Ok(Js::str(&format!("x={}", args[0].get("x")?.to_display())))
                    }),
                ),
            ]))
            .await
            .unwrap();
        assert_eq!(
            signer.calls.borrow()[0]["config"]["params"],
            json!({"x":"unsigned"})
        );
        assert_eq!(seen.borrow()[0].url, "/private?x=signed");
        assert!(seen.borrow()[0].params.is_none());
        let config = response.get("config").unwrap();
        assert_eq!(config.get("url").unwrap().to_display(), "/private");
        assert_eq!(
            to_json(&config.get("params").unwrap()).unwrap(),
            json!({"x":"signed"})
        );
        assert_eq!(
            config
                .get("headers")
                .unwrap()
                .get("x-relay-signature")
                .unwrap()
                .to_display(),
            hmac_hex("GET /private")
        );
    });
}
