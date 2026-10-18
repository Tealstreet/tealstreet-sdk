//! Hosted GRVT HTTP plumbing through the native registry; SDK orders still need a wallet.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct HostedGrvt {
    calls: RefCell<Vec<Value>>,
    reject: Cell<bool>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedGrvt {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP-only witness")
        };
        let mut headers = request.config.headers;
        headers.insert("X-Hosted-GRVT".into(), "callback-proof".into());
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: (!self.reject.get()).then_some(SignedConfig {
                url: None,
                params: request.config.params,
                data: request.config.data,
                headers,
            }),
            error: self.reject.get().then(|| "hosted callback refused".into()),
        });
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for HostedGrvt {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
#[test]
fn grvt_native_host_signs_http_without_a_local_wallet_and_keeps_sdk_order_guard() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/grvt_hosted_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["calls"].as_array().unwrap().len(),
        3,
        "three actual source factories"
    );
    support::run_local(async {
        let signer = Rc::new(HostedGrvt::default());
        let h=support::venue_harness(ExchangeName::Grvt,Rc::new(|request|Some(match request.path.as_str() {
            "/lite/v1/all_instruments"=>json!({"r":[{"i":"BTC_USDT_Perp","k":"PERPETUAL","b":"BTC","q":"USDT","ms":"0.001","ts":"0.1","ih":"0x030501","bd":3,"qd":6}]}),
            "/lite/v1/account_summary"=>json!({"r":{}}),
            path=>panic!("unexpected GRVT route {path}"),
        })),json!({"key":"hosted-api-key","secret":"","grvtTradingAccountId":"42"}),Some((signer.clone(),Some("grvt-hosted-account".into()))),safe_cex_exchange::native::native_runtime()).await;
        assert!(h.exchange.base().options().secret.is_empty());
        assert!(h.exchange.base().signers.remote_signer.is_some());
        assert_eq!(
            h.exchange.fetch_markets().await.unwrap()[0].symbol,
            "BTC_USDT_PERP"
        );
        assert_eq!(h.exchange.get_account().await.unwrap().user_id, "");
        {
            let calls = signer.calls.borrow();
            assert_eq!(
                calls.len(),
                2,
                "public and private HTTP both use source signer"
            );
            assert_eq!(
                calls[0]["config"]["baseURL"],
                source["calls"][0]["config"]["baseURL"]
            );
            assert_eq!(calls[0]["config"]["url"], "/lite/v1/all_instruments");
            for field in ["exchangeName", "accountName", "config"] {
                assert_eq!(
                    calls[1][field], source["calls"][2][field],
                    "source/native hosted {field}"
                );
            }
        }
        {
            let seen = h.seen.borrow();
            assert_eq!(seen.len(), 2);
            assert!(
                seen.iter()
                    .all(|r| r.headers["x-hosted-grvt"] == "callback-proof"),
                "signer headers reach actual reqwest wire"
            );
            assert_eq!(seen[1].method, "POST");
            assert_eq!(
                serde_json::from_str::<Value>(&seen[1].body).unwrap(),
                json!({"sub_account_id":""})
            );
        }
        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTC_USDT_PERP","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let error = h
            .exchange
            .base()
            .place_order(h.exchange.as_ref(), order)
            .await
            .unwrap_err();
        assert_eq!(
            error.message,
            source["walletError"].as_str().unwrap(),
            "source SDK wallet guard"
        );
        assert_eq!(
            signer.calls.borrow().len(),
            2,
            "no RPC for un-signable SDK order"
        );
        assert_eq!(
            h.seen.borrow().len(),
            2,
            "no HTTP for un-signable SDK order"
        );
        signer.reject.set(true);
        let error = h.exchange.get_account().await.unwrap_err();
        assert!(
            error.message.contains("hosted callback refused"),
            "{}",
            error.message
        );
        assert_eq!(signer.calls.borrow().len(), 3);
        assert_eq!(h.seen.borrow().len(), 2, "rejected signer cannot send");
        h.exchange.dispose().unwrap();
    });
}
