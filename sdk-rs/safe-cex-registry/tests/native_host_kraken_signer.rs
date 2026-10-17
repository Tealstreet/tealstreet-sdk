//! Both Kraken names use the shared hosted-account bridge with no local secret.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{cell::RefCell, rc::Rc};

const NONCE: &str = "1700000000000123";
const KEY: &str = "hosted-kraken-key";
const ACCOUNT: &str = "hosted-kraken-account";
const RELAY_SECRET: &[u8] = b"secret held only by the browser relay";

fn signature(spot: bool, path: &str, query_or_body: &str, nonce: &str) -> String {
    let payload = if spot {
        let hash = Sha256::digest(format!("{nonce}{query_or_body}").as_bytes());
        [path.as_bytes(), hash.as_slice()].concat()
    } else {
        Sha256::digest(format!("{query_or_body}{nonce}{path}").as_bytes()).to_vec()
    };
    let mut mac = Hmac::<Sha512>::new_from_slice(RELAY_SECRET).unwrap();
    mac.update(&payload);
    STANDARD.encode(mac.finalize().into_bytes())
}

#[derive(Default)]
struct BrowserRelay {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for BrowserRelay {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP witness requested WS signing")
        };
        let spot = request.exchange_name == ExchangeName::Krakenspot;
        let mut headers = request.config.headers;
        // Private configs reach the relay unsigned, before the native terminal adapter.
        assert!(
            !headers
                .keys()
                .any(|k| matches!(k.to_ascii_lowercase().as_str(), "authent" | "api-sign"))
        );
        assert_eq!(request.account_name.as_deref(), Some(ACCOUNT));
        let (path, body) = if spot {
            assert_eq!(request.config.url, "/0/private/Balance");
            assert_eq!(
                request.config.data,
                Some(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({}))
                        .unwrap()
                )
            );
            (request.config.url, format!("nonce={NONCE}"))
        } else {
            assert_eq!(request.config.url, "/v3/accounts");
            (format!("/api{}", request.config.url), String::new())
        };
        if spot {
            headers.insert("API-Key".into(), KEY.into());
            headers.insert("API-Sign".into(), signature(true, &path, &body, NONCE));
        } else {
            headers.insert("APIKey".into(), KEY.into());
            headers.insert("Authent".into(), signature(false, &path, "", NONCE));
            headers.insert("Nonce".into(), NONCE.into());
        }
        headers.insert(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        );
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: Some(SignedConfig {
                url: None,
                params: None,
                data: spot.then(|| {
                    safe_cex_types::retained_json::RetainedJsonInput::Text(
                        body.encode_utf16().collect(),
                    )
                }),
                headers,
            }),
            error: None,
        });
        Box::pin(async { Ok(response) })
    }
}
impl RemoteSigner for BrowserRelay {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

fn check(name: ExchangeName) {
    support::run_local(async move {
        let spot = name == ExchangeName::Krakenspot;
        let relay = Rc::new(BrowserRelay::default());
        let h = support::venue_harness(name.clone(), Rc::new(move |request| match request.path.as_str() {
            "/derivatives/api/v3/accounts" => Some(json!({"result":"success","accounts":{"flex":{
                "type":"multiCollateralMarginAccount","portfolioValue":1000,"availableMargin":800,
                "totalUnrealized":5,"currencies":{"USD":{"quantity":1000,"value":1000,"available":800}}
            }}})),
            "/0/private/Balance" => Some(json!({"error":[],"result":{"ZUSD":"7"}})),
            _ => None,
        }), json!({"key":KEY,"secret":""}), Some((relay.clone(), Some(ACCOUNT.into()))),
            safe_cex_exchange::native::native_runtime()).await;
        assert_eq!(h.exchange.name(), &name);
        assert!(
            h.exchange
                .base()
                .signers
                .remote_signer
                .as_ref()
                .unwrap()
                .is_available()
        );
        let validation = h
            .exchange
            .validate_account()
            .await
            .expect("native hosted Kraken validation");
        assert_eq!(validation.error.as_value(), &json!(""));
        let balance = validation
            .balance
            .expect("validation returns the private balance");
        assert_eq!(
            (balance.total, balance.free),
            if spot { (7.0, 7.0) } else { (1000.0, 800.0) }
        );
        assert_eq!(h.store.balance().total, 0.0, "fetchBalance is pure");
        let calls = relay.calls.borrow();
        assert_eq!(
            calls.len(),
            2,
            "both private HTTP requests cross the hosted signer"
        );
        assert_eq!(
            calls[0]["exchangeName"],
            serde_json::to_value(&name).unwrap()
        );
        assert_eq!(calls[0]["accountName"], ACCOUNT);
        assert_eq!(
            calls[0]["config"]["method"],
            if spot { "POST" } else { "GET" }
        );
        let seen = h.seen.borrow();
        assert_eq!(seen.len(), 2);
        let request = &seen[0];
        if spot {
            assert_eq!(request.path, "/0/private/Balance");
            assert_eq!(request.body, format!("nonce={NONCE}"));
            assert_eq!(request.headers["api-key"], KEY);
            // Verify the signature independently from the relay using observed wire bytes.
            assert_eq!(
                request.headers["api-sign"],
                signature(true, &request.path, &request.body, NONCE)
            );
        } else {
            assert_eq!(request.path, "/derivatives/api/v3/accounts");
            assert_eq!(request.headers["apikey"], KEY);
            assert_eq!(request.headers["nonce"], NONCE);
            let path = request.path.strip_prefix("/derivatives").unwrap();
            assert_eq!(
                request.headers["authent"],
                signature(false, path, &request.query, &request.headers["nonce"])
            );
        }
        h.exchange.dispose().expect("dispose Kraken");
    });
}

#[test]
fn kraken_native_host_signs_with_a_hosted_account_and_no_local_secret() {
    check(ExchangeName::Kraken);
}
#[test]
fn krakenspot_native_host_signs_with_a_hosted_account_and_no_local_secret() {
    check(ExchangeName::Krakenspot);
}

#[derive(Default)]
struct StartupRelay {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for StartupRelay {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let response = match request {
            RemoteSignerRequest::Http(request) => RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                config: Some(SignedConfig {
                    headers: request.config.headers,
                    ..Default::default()
                }),
                error: None,
            }),
            RemoteSignerRequest::WsAuth { request, .. } => {
                RemoteSignerResponse::WsAuth(safe_cex_types::ws_auth::WsAuthResponse {
                    id: request.id,
                    ws_auth_signature: Some("relay-challenge-signature".into()),
                    error: None,
                })
            }
        };
        Box::pin(async { Ok(response) })
    }
}
impl RemoteSigner for StartupRelay {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn kraken_native_private_socket_uses_the_hosted_account_for_challenge_signing() {
    support::run_local(async {
        let relay = Rc::new(StartupRelay::default());
        let h = support::venue_harness(ExchangeName::Kraken, Rc::new(|r| match r.path.as_str() {
            "/derivatives/api/v3/instruments" => Some(json!({"result":"success","instruments":[{
                "symbol":"PF_XBTUSD","tradeable":true,"base":"BTC","quote":"USD",
                "contractValueTradePrecision":4,"tickSize":0.5,"contractSize":1,
                "marginLevels":[{"initialMargin":0.02}],"contractType":"flexible_futures","status":"open"
            }]})),
            "/derivatives/api/v3/tickers" => Some(json!({"result":"success","tickers":[{
                "symbol":"PF_XBTUSD","bid":30000,"ask":30001,"last":30000.5,
                "markPrice":30000.25,"indexPrice":30000.1,"vol24h":12,"openInterest":7,
                "fundingRate":0.0001,"change24h":1.5
            }]})),
            "/derivatives/api/v3/leveragepreferences" => Some(json!({"result":"success","leveragePreferences":[]})),
            "/derivatives/api/v3/openorders" => Some(json!({"result":"success","openOrders":[]})),
            "/derivatives/api/v3/openpositions" => Some(json!({"result":"success","openPositions":[]})),
            "/derivatives/api/v3/accounts" => Some(json!({"result":"success","accounts":{}})),
            _ => None,
        }), json!({"key":KEY,"secret":""}), Some((relay.clone(), Some(ACCOUNT.into()))),
            safe_cex_exchange::native::native_runtime()).await;
        tokio::time::timeout(std::time::Duration::from_secs(5), h.exchange.start())
            .await
            .expect("Kraken native start deadline")
            .expect("Kraken native hosted start");
        support::eventually("private challenge request", || {
            h.received
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"event\":\"challenge\""))
        })
        .await;
        assert!(
            !h.received
                .borrow()
                .iter()
                .any(|frame| frame.contains("signed_challenge")),
            "no private subscription before challenge signing"
        );
        for peer in h.peers.borrow().iter() {
            peer.push
                .send(json!({"event":"challenge","message":"hosted-challenge"}).to_string())
                .unwrap();
        }
        support::eventually("remote-signed private subscription", || {
            h.received
                .borrow()
                .iter()
                .filter(|frame| frame.contains("relay-challenge-signature"))
                .count()
                == 4
        })
        .await;
        let calls = relay.calls.borrow();
        let ws = calls
            .iter()
            .find(|call| call["type"] == "ws-auth")
            .expect("WS auth reaches hosted signer");
        assert_eq!(ws["exchangeName"], "kraken");
        assert_eq!(
            ws["accountName"], ACCOUNT,
            "native WS auth retains hosted account metadata"
        );
        assert_eq!(ws["wsAuth"]["extraData"]["challenge"], "hosted-challenge");
        assert_eq!(ws["wsAuth"]["toSign"], "hosted-challenge");
        assert_eq!(ws["wsAuth"]["outputFormat"], "kraken-challenge");
        let frames: Vec<Value> = h
            .received
            .borrow()
            .iter()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .filter(|frame| frame["signed_challenge"] == "relay-challenge-signature")
            .collect();
        assert_eq!(
            frames.len(),
            4,
            "all four private feeds wait for remote signing"
        );
        for frame in frames {
            assert_eq!(frame["event"], "subscribe");
            assert_eq!(frame["api_key"], KEY);
            assert_eq!(frame["original_challenge"], "hosted-challenge");
        }

        h.exchange.dispose().expect("dispose Kraken sockets");
    });
}
