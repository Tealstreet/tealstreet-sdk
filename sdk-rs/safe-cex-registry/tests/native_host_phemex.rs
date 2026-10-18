//! Phemex through the Rust registry, production native inputs and real local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;

#[test]
fn phemex_registry_constructs_native_inputs_and_signs_account_http() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Phemex,
            Rc::new(|request| match request.path.as_str() {
                "/public/products" => Some(json!({"data":{"perpProductsV2":[{
                    "symbol":"BTCUSDT","type":"PerpetualV2","baseCurrency":"BTC",
                    "quoteCurrency":"USDT","settleCurrency":"USDT","qtyStepSize":"0.003",
                    "tickSize":"0.25","maxOrderQtyRq":"100","maxLeverage":"50"
                }]}})),
                "/g-accounts/accountPositions" => Some(json!({"code":0,"data":{
                    "account":{"userID":123,"accountBalanceRv":"1000","totalUsedBalanceRv":"200"},
                    "positions":[]
                }})),
                _ => None,
            }),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.base().name,
            ExchangeName::Phemex,
            "REGISTRY Phemex identity"
        );
        let markets = h
            .exchange
            .fetch_markets()
            .await
            .expect("Phemex native market read");
        assert_eq!(markets.len(), 1, "REGISTRY Phemex market census");
        assert_eq!(markets[0].symbol, "BTCUSDT");
        assert_eq!(
            (markets[0].precision.amount, markets[0].precision.price),
            (0.003, 0.25)
        );
        assert!(h.store.markets().is_empty(), "fetchMarkets read purity");
        let account = h
            .exchange
            .get_account()
            .await
            .expect("Phemex native signed account read");
        assert_eq!(serde_json::to_value(account).unwrap()["userId"], "123");
        let seen = h.seen.borrow();
        let request = seen
            .iter()
            .find(|request| request.path == "/g-accounts/accountPositions")
            .expect("REGISTRY Phemex account reaches real local HTTP");
        assert_eq!(request.method, "GET");
        assert_eq!(request.query, "currency=USDT");
        assert_eq!(request.headers["x-phemex-access-token"], "native-key");
        let expiry = &request.headers["x-phemex-request-expiry"];
        let payload = format!("/g-accounts/accountPositionscurrency=USDT{expiry}");
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(request.headers["x-phemex-request-signature"], expected);
        h.exchange.dispose().expect("Phemex dispose");
    });
}

#[derive(Default)]
struct PendingPhemexSigner {
    release: tokio::sync::Notify,
    calls: std::cell::RefCell<Vec<serde_json::Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for PendingPhemexSigner {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let safe_cex_transport::signing::RemoteSignerRequest::WsAuth { request, .. } = request
        else {
            panic!("private-only Phemex witness must request websocket auth");
        };
        Box::pin(async move {
            self.release.notified().await;
            Ok(safe_cex_transport::signing::RemoteSignerResponse::WsAuth(
                safe_cex_types::ws_auth::WsAuthResponse {
                    id: request.id,
                    ws_auth_signature: Some("hosted-native-signature".into()),
                    error: None,
                },
            ))
        })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for PendingPhemexSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn native_phemex_auth_resumes_on_signer_wake_and_subscribes_only_after_ack() {
    support::run_local(async {
        let signer = Rc::new(PendingPhemexSigner::default());
        let h = support::venue_harness(
            ExchangeName::Phemex,
            Rc::new(|_| None),
            json!({"key":"native-key","secret":""}),
            Some((signer.clone(), Some("native-phemex-account".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange
            .base()
            .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
            .expect("constructor registered the actual Phemex private socket")
            .connect_and_subscribe(false)
            .unwrap();
        support::eventually("real Phemex connection awaiting hosted signature", || {
            !signer.calls.borrow().is_empty() && !h.peers.borrow().is_empty()
        })
        .await;
        let frames = || {
            h.received
                .borrow()
                .iter()
                .map(|frame| serde_json::from_str::<serde_json::Value>(frame).unwrap())
                .collect::<Vec<_>>()
        };
        assert!(!frames().iter().any(|frame| frame["method"] == "user.auth" || frame["method"] == "aop_p.subscribe"));
        assert_eq!(signer.calls.borrow().len(), 1);
        let call = signer.calls.borrow()[0].clone();
        assert_eq!(call["exchangeName"], "phemex");
        assert_eq!(call["accountName"], "native-phemex-account");
        assert_eq!(call["wsAuth"]["outputFormat"], "hex");
        assert!(
            call["wsAuth"]["toSign"]
                .as_str()
                .unwrap()
                .starts_with("native-key")
        );
        signer.release.notify_one();
        support::eventually("Phemex user.auth after real signer IO wake", || {
            frames().iter().any(|frame| frame["method"] == "user.auth")
        })
        .await;
        let sent = frames();
        let auth = sent
            .iter()
            .find(|frame| frame["method"] == "user.auth")
            .unwrap();
        assert_eq!(auth["id"], 999);
        assert_eq!(auth["params"][0], "API");
        assert_eq!(auth["params"][1], "native-key");
        assert_eq!(auth["params"][2], "hosted-native-signature");
        assert!(
            !sent
                .iter()
                .any(|frame| frame["method"] == "aop_p.subscribe")
        );
        h.peers.borrow()[0]
            .push
            .send(json!({"id":999,"error":null,"result":{"status":"success"}}).to_string())
            .unwrap();
        support::eventually(
            "source aop_p subscription after auth acknowledgement",
            || {
                frames()
                    .iter()
                    .any(|frame| frame["method"] == "aop_p.subscribe")
            },
        )
        .await;
        assert_eq!(
            frames()
                .iter()
                .filter(|frame| frame["method"] == "user.auth")
                .count(),
            1
        );
        assert_eq!(
            frames()
                .iter()
                .filter(|frame| frame["method"] == "aop_p.subscribe")
                .count(),
            1
        );
        h.exchange.dispose().unwrap();
    });
}
