//! Rust registry -> real Apex binding -> native loopback HTTP/WS and signer.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

fn routes(request: &Seen) -> Option<Value> {
    let path = request.path.strip_prefix("/api").unwrap_or(&request.path);
    let data = match (request.method.as_str(), path) {
        ("GET", "/v3/symbols") => {
            json!({"contractConfig":{"perpetualContract":[{"symbol":"BTC-USDC","crossSymbolName":"BTCUSDC","baseTokenId":"BTC","settleAssetId":"USDC","l2PairId":"11","enableTrade":true,"stepSize":"0.001","tickSize":"0.01","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"50","initialMarginRate":"0.1"}]}})
        }
        ("GET", "/v3/account") => {
            json!({"id":"16909060","positions":[],"l2key":"native-l2","contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}})
        }
        ("GET", "/v3/account-balance") => {
            json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"})
        }
        ("GET", "/v3/open-orders") => json!([]),
        ("GET", "/v3/user") => json!({"id":"16909060"}),
        ("POST", "/v3/order") => json!({"id":"9007199254740993"}),
        ("POST", "/v3/delete-order") => json!({}),
        _ => return None,
    };
    Some(json!({"code":0,"data":data}))
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        result.push(TABLE[((word >> 18) & 63) as usize] as char);
        result.push(TABLE[((word >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            TABLE[((word >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            TABLE[(word & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}
#[test]
fn apexomni_native_registry_bootstraps_signs_places_and_cancels() {
    run_local(async {
        let h=venue_harness(ExchangeName::Apexomni,Rc::new(routes),json!({"key":"native-key","secret":SECRET,"passphrase":"native-passphrase","omniKeySeed":format!("0x{}1b","01".repeat(64)),"tealstreetTunnel":false}),None,safe_cex_exchange::native::native_runtime()).await;
        h.exchange.start().await.expect("Apex native start");
        let loaded = h.store.loaded().clone();
        assert!(
            loaded.markets && loaded.tickers && loaded.balance && loaded.orders && loaded.positions
        );
        eventually("Apex native private login", || {
            h.received
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"op\":\"login\""))
        })
        .await;
        assert_eq!(h.exchange.get_account().await.unwrap().user_id, "16909060");
        let opts: PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tsnativecaller"})).unwrap();
        assert_eq!(
            h.exchange
                .base()
                .place_order(h.exchange.as_ref(), opts)
                .await
                .unwrap(),
            ["9007199254740993"]
        );
        let order = json!({"id":"9007199254740993","symbol":"BTC-USDC","type":"LIMIT","side":"BUY","status":"OPEN","timeInForce":"GOOD_TIL_CANCEL","price":"100.13","size":"1.235","cumSuccessFillSize":"0","reduceOnly":false,"triggerPrice":"0","updatedAt":0,"openSlParams":{"triggerPrice":"0"},"openTpParams":{"triggerPrice":"0"}});
        let peer = h
            .peers
            .borrow()
            .iter()
            .find(|p| p.path.contains("private"))
            .unwrap()
            .push
            .clone();
        let received_order = Rc::new(std::cell::RefCell::new(None));
        let observed = received_order.clone();
        let cleanup = h.store.subscribe(Box::new(move |data, _| {
            if let Some(order) = data.orders.iter().find(|o| o.id == "9007199254740993") {
                *observed.borrow_mut() = Some(order.clone());
            }
            Ok(())
        }));
        peer.send(json!({"topic":"ws_zk_accounts_v3","contents":{"orders":[order]}}).to_string())
            .unwrap();
        eventually("Apex placed order arrives through private WS", || {
            received_order.borrow().is_some()
        })
        .await;
        let order = received_order
            .borrow()
            .clone()
            .expect("private WS delivered order");
        cleanup();
        h.exchange.cancel_orders(vec![order]).await.unwrap();
        let seen = h.seen.borrow();
        let placed = seen
            .iter()
            .find(|r| r.method == "POST" && r.path == "/api/v3/order")
            .unwrap();
        assert!(placed.body.contains("clientOrderId=tsnativecaller"));
        assert!(placed.body.contains("size=1.235"));
        assert!(placed.body.contains("price=100.13"));
        let signature = placed
            .body
            .split('&')
            .find_map(|v| v.strip_prefix("signature="))
            .unwrap();
        assert_eq!(signature.len(), 128);
        let timestamp = placed.headers.get("apex-timestamp").unwrap();
        let message = format!("{timestamp}POST/api/v3/order{}", placed.body);
        use hmac::{Hmac, Mac};
        let mut mac =
            Hmac::<sha2::Sha256>::new_from_slice(base64(SECRET.as_bytes()).as_bytes()).unwrap();
        mac.update(message.as_bytes());
        assert_eq!(
            placed.headers.get("apex-signature").unwrap(),
            &base64(&mac.finalize().into_bytes()),
            "HMAC authenticates actual transmitted form bytes"
        );
        assert!(seen.iter().any(|r| r.method == "POST"
            && r.path == "/api/v3/delete-order"
            && r.body.contains("9007199254740993")));
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}

// Browser-side fixture: the API secret exists only on this side of the relay.
#[derive(Default)]
struct HostedApex {
    calls: std::cell::RefCell<Vec<Value>>,
    ws_gate: std::cell::RefCell<Option<tokio::sync::oneshot::Receiver<()>>>,
    ws_mode: std::cell::Cell<u8>,
    reject_http: std::cell::Cell<bool>,
    dispose_on_ws: std::cell::RefCell<Option<Box<dyn Fn()>>>,
}
fn hosted_apex_mac(message: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(base64(SECRET.as_bytes()).as_bytes()).unwrap();
    mac.update(message.as_bytes());
    base64(&mac.finalize().into_bytes())
}
impl safe_cex_transport::signing::RemoteSigner for HostedApex {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        use safe_cex_transport::signing::{
            HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig,
        };
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        match request {
            RemoteSignerRequest::Http(request) => {
                assert_eq!(request.exchange_name, ExchangeName::Apexomni);
                let response =
                    if self.reject_http.get() {
                        HttpSigningResponse {
                            id: request.id,
                            config: None,
                            error: Some("fixture HTTP signer unavailable".into()),
                        }
                    } else {
                        let config = request.config;
                        let timestamp =
                            (1_700_000_000_000u64 + self.calls.borrow().len() as u64).to_string();
                        let method = config.method.to_uppercase();
                        let query_method = method == "GET" || method == "DELETE";
                        // These fixture reads have no parameters. Placements arrive already
                        // serialized by the actual venue API, as they do in signHttpRequest.
                        assert!(config.params.as_ref().is_none_or(
                            |p| p.is_null() || p.as_object().is_some_and(|o| o.is_empty())
                        ));
                        let data = config.data.as_ref().map_or_else(String::new, |v| match v {
                            safe_cex_types::retained_json::RetainedJsonInput::Text(units) => {
                                String::from_utf16(units).expect("Apex signed form string")
                            }
                            _ => panic!("Apex signed form string"),
                        });
                        let path = format!(
                            "/api{}",
                            config.url.strip_prefix("/api").unwrap_or(&config.url)
                        );
                        let message = format!(
                            "{timestamp}{method}{path}{}",
                            if query_method { "" } else { &data }
                        );
                        let mut headers = config.headers;
                        headers.insert("APEX-SIGNATURE".into(), hosted_apex_mac(&message));
                        headers.insert("APEX-TIMESTAMP".into(), timestamp);
                        headers.insert("APEX-API-KEY".into(), "native-key".into());
                        headers.insert("APEX-PASSPHRASE".into(), "native-passphrase".into());
                        HttpSigningResponse {
                            id: request.id,
                            error: None,
                            config: Some(SignedConfig {
                                headers,
                                data: config.data,
                                params: config.params,
                                ..Default::default()
                            }),
                        }
                    };
                Box::pin(async move { Ok(RemoteSignerResponse::Http(response)) })
            }
            RemoteSignerRequest::WsAuth {
                exchange_name,
                request,
                ..
            } => {
                assert_eq!(exchange_name, ExchangeName::Apexomni);
                let auth = request.ws_auth.unwrap();
                assert_eq!(auth.output_format, Some(json!("apexomni-base64")));
                let extra: Value =
                    serde_json::from_str(auth.extra_data.as_ref().unwrap().get()).unwrap();
                let message = auth.to_sign.unwrap();
                assert_eq!(
                    message,
                    format!("{}GET/ws/accounts", extra["timestamp"].as_f64().unwrap())
                );
                let dispose = self.dispose_on_ws.borrow_mut().take();
                if let Some(dispose) = dispose {
                    dispose();
                }
                let gate = self.ws_gate.borrow_mut().take();
                let mode = self.ws_mode.get();
                Box::pin(async move {
                    if let Some(gate) = gate {
                        gate.await.unwrap();
                    }
                    if mode == 2 {
                        return Err(safe_cex_transport::TransportError::Host(
                            "fixture WS transport unavailable".into(),
                        ));
                    }
                    Ok(RemoteSignerResponse::WsAuth(
                        safe_cex_types::ws_auth::WsAuthResponse {
                            id: request.id,
                            ws_auth_signature: (mode == 0).then(|| hosted_apex_mac(&message)),
                            error: (mode == 1).then(|| "fixture WS signer unavailable".into()),
                        },
                    ))
                })
            }
        }
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for HostedApex {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}
fn hosted_apex_options() -> Value {
    json!({"key":"native-key","secret":"","passphrase":"native-passphrase","omniKeySeed":format!("0x{}1b","01".repeat(64)),"tealstreetTunnel":false})
}
fn private_frame(frame: &str) -> bool {
    frame.contains("\"op\":\"login\"") || frame.contains("ws_zk_accounts_v3")
}
#[test]
fn apexomni_native_hosted_signer_authenticates_http_and_ws() {
    run_local(async {
        for account in [Some("hosted-apex".to_string()), None] {
            let signer = Rc::new(HostedApex::default());
            let (release, gate) = tokio::sync::oneshot::channel();
            *signer.ws_gate.borrow_mut() = Some(gate);
            let h = venue_harness(
                ExchangeName::Apexomni,
                Rc::new(routes),
                hosted_apex_options(),
                Some((signer.clone(), account.clone())),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert!(
                h.exchange.can_sign(),
                "hosted signer participates in BaseExchange canSign"
            );
            assert!(h.exchange.base().options().secret.is_empty());
            h.exchange
                .start()
                .await
                .expect("Apex starts through hosted HTTP signing");
            eventually("Apex requests remote WS auth", || {
                signer.calls.borrow().iter().any(|r| r["type"] == "ws-auth")
            })
            .await;
            assert!(
                !h.received.borrow().iter().any(|f| private_frame(f)),
                "private login and subscription await the hosted signature"
            );
            release.send(()).unwrap();
            eventually("hosted Apex private subscription", || {
                h.received
                    .borrow()
                    .iter()
                    .any(|f| f.contains("ws_zk_accounts_v3"))
            })
            .await;
            {
                let frames = h.received.borrow();
                let login_index = frames
                    .iter()
                    .position(|f| f.contains("\"op\":\"login\""))
                    .unwrap();
                let subscribe_index = frames
                    .iter()
                    .position(|f| f.contains("ws_zk_accounts_v3"))
                    .unwrap();
                assert!(login_index < subscribe_index);
                let envelope: Value = serde_json::from_str(&frames[login_index]).unwrap();
                let login: Value =
                    serde_json::from_str(envelope["args"][0].as_str().unwrap()).unwrap();
                assert_eq!(login["apiKey"], "native-key");
                assert_eq!(login["passphrase"], "native-passphrase");
                let message = format!("{}GET/ws/accounts", login["timestamp"].as_f64().unwrap());
                assert_eq!(
                    login["signature"],
                    hosted_apex_mac(&message),
                    "remote HMAC authenticates the transmitted private login"
                );
            }
            let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tshostedcaller"})).unwrap();
            assert_eq!(
                h.exchange
                    .base()
                    .place_order(h.exchange.as_ref(), opts)
                    .await
                    .unwrap(),
                ["9007199254740993"]
            );
            let seen = h.seen.borrow();
            let placed = seen
                .iter()
                .find(|r| r.method == "POST" && r.path == "/api/v3/order")
                .unwrap();
            assert!(
                placed.body.contains("clientOrderId=tshostedcaller")
                    && placed.body.contains("size=1.235")
                    && placed.body.contains("price=100.13")
            );
            assert_eq!(
                placed.headers.get("apex-signature").unwrap(),
                &hosted_apex_mac(&format!(
                    "{}POST/api/v3/order{}",
                    placed.headers.get("apex-timestamp").unwrap(),
                    placed.body
                )),
                "hosted HMAC authenticates the actual transmitted order body"
            );
            drop(seen);
            let calls = signer.calls.borrow();
            assert!(
                calls
                    .iter()
                    .all(|r| r.get("accountName").and_then(Value::as_str) == account.as_deref()),
                "HTTP and WS retain the hosted account, including absence"
            );
            assert!(
                calls.iter().all(|r| !r.to_string().contains(SECRET)),
                "browser secret never enters the exchange"
            );
            drop(calls);
            h.exchange.dispose().unwrap();
        }
    });
}
#[test]
fn apexomni_native_hosted_signer_refusal_sends_no_unsigned_request() {
    run_local(async {
        let signer = Rc::new(HostedApex::default());
        signer.reject_http.set(true);
        let h = venue_harness(
            ExchangeName::Apexomni,
            Rc::new(routes),
            hosted_apex_options(),
            Some((signer, Some("hosted-apex".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        let error = h.exchange.get_account().await.unwrap_err();
        assert!(
            error.message.contains("fixture HTTP signer unavailable"),
            "{error:?}"
        );
        assert!(
            h.seen.borrow().is_empty(),
            "signer refusal sends no unsigned request"
        );
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn apexomni_native_hosted_ws_refusal_and_throw_send_no_private_frames() {
    run_local(async {
        for mode in [1, 2] {
            let signer = Rc::new(HostedApex::default());
            signer.ws_mode.set(mode);
            let h = venue_harness(
                ExchangeName::Apexomni,
                Rc::new(routes),
                hosted_apex_options(),
                Some((signer.clone(), Some("hosted-apex".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            let errors = Rc::new(std::cell::RefCell::new(Vec::new()));
            let observed = errors.clone();
            h.exchange.base().on(
                "error",
                Rc::new(move |_, payload| {
                    observed.borrow_mut().push(payload.to_string());
                    Ok(())
                }),
            );
            h.exchange.start().await.unwrap();
            eventually("refused Apex WS signing is reported", || {
                errors.borrow().iter().any(|e| e.contains("fixture WS"))
            })
            .await;
            assert!(signer.calls.borrow().iter().any(|r| r["type"] == "ws-auth"));
            assert!(
                errors
                    .borrow()
                    .iter()
                    .all(|e| !e.to_lowercase().contains("auth failed")),
                "relay failure must not classify credentials as invalid"
            );
            if mode == 1 {
                assert!(
                    errors
                        .borrow()
                        .iter()
                        .any(|e| e.contains("Remote WS signing unavailable"))
                );
            }
            assert!(
                !h.received.borrow().iter().any(|f| private_frame(f)),
                "refused or thrown hosted WS signing cannot log in or subscribe"
            );
            h.exchange.dispose().unwrap();
        }
    });
}
#[test]
fn apexomni_native_dispose_cancels_pending_hosted_ws_auth() {
    run_local(async {
        for reentrant in [false, true] {
            let signer = Rc::new(HostedApex::default());
            let (release, gate) = tokio::sync::oneshot::channel();
            *signer.ws_gate.borrow_mut() = Some(gate);
            let h = venue_harness(
                ExchangeName::Apexomni,
                Rc::new(routes),
                hosted_apex_options(),
                Some((signer.clone(), Some("hosted-apex".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            if reentrant {
                let weak = Rc::downgrade(&h.exchange);
                *signer.dispose_on_ws.borrow_mut() = Some(Box::new(move || {
                    weak.upgrade().unwrap().dispose().unwrap();
                }));
            }
            h.exchange.start().await.unwrap();
            eventually("pending hosted Apex auth", || {
                signer.calls.borrow().iter().any(|r| r["type"] == "ws-auth")
            })
            .await;
            if !reentrant {
                h.exchange.dispose().unwrap();
            }
            // Keep the wrapper alive: dropping it must not hide work that was
            // registered after a reentrant disposal during the eager first poll.
            eventually("hosted WS signing future is canceled", || {
                release.is_closed()
            })
            .await;
            assert!(
                !h.received.borrow().iter().any(|f| private_frame(f)),
                "disposing pending auth sends no private frames"
            );
            let weak = Rc::downgrade(&h.exchange);
            drop(h.exchange);
            eventually("disposed Apex exchange releases", || {
                weak.upgrade().is_none()
            })
            .await;
        }
    });
}
