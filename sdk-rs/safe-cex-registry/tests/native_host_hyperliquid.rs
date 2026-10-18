//! Registry construction and real local HTTP through the production Hyperliquid
//! host, including secretless browser signing and transport-selected base URLs.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_registry::{
    native_host::{hyperliquid_rate_limiter, native_venue_host_signed, native_venue_host_with},
    rust_venues::{RustVenueHost, rust_constructor},
};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const ADDRESS: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";

#[test]
fn hyperliquid_registry_folds_journal_records_over_native_http() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Hyperliquid,
            Rc::new(|request| {
                let body: Value = serde_json::from_str(&request.body).unwrap();
                assert_eq!(request.path, "/info");
                assert_eq!(body["user"], ADDRESS);
                match body["type"].as_str().unwrap() {
                    "userFillsByTime" => Some(json!([
                        {"coin":"BTC","tid":1,"oid":10,"time":1699999998000u64,"side":"B","px":"61000","sz":"1","fee":"1","feeToken":"USDC","closedPnl":"0","dir":"Open Long"},
                        {"coin":"BTC","tid":2,"oid":11,"time":1699999999000u64,"side":"A","px":"62000","sz":"1","fee":"2","feeToken":"USDC","closedPnl":"1000","dir":"Close Long"}
                    ])),
                    "userFunding" => Some(json!([])),
                    other => panic!("unexpected journal request {other}"),
                }
            }),
            json!({"key":ADDRESS, "secret":"", "applicationId":ADDRESS}),
            None,
            safe_cex_exchange::native::native_runtime(),
        ).await;
        h.store
            .update(
                safe_cex_store::StoreUpdate {
                    markets: Some(vec![serde_json::from_value(json!({
                "id":"BTC", "symbol":"BTC-PERP", "base":"BTC", "quote":"USDC", "active":true,
                "precision":{"amount":0.00001,"price":1},
                "limits":{"amount":{"min":0.00001,"max":100},"leverage":{"min":1,"max":40}}
            })).unwrap()]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        let records = h
            .exchange
            .fetch_historical_positions(safe_cex_exchange::journal::HistoricalPositionOptions {
                account: "native-journal".into(),
                from: Some(1_699_999_000_000.0),
                to: Some(1_700_000_000_000.0),
            })
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        let record: safe_cex_types::PositionRecord =
            serde_json::from_value(records[0].clone()).unwrap();
        assert_eq!(record.account, "native-journal");
        assert_eq!(record.symbol, "BTC-PERP");
        assert_eq!(record.settle_currency, "USDC");
        assert_eq!(record.realized_pnl, 1000.0);
        assert_eq!(record.fees, 3.0);
        assert!(
            h.seen.borrow().len() >= 3,
            "real income and execution reads reached HTTP"
        );
        assert!(
            h.peers.borrow().is_empty(),
            "pure journal reads do not start sockets"
        );
        h.exchange.dispose().unwrap();
    });
}

fn sign(body: &Value, mainnet: bool) -> Value {
    serde_json::to_value(
        safe_cex_venue_hyperliquid::signing::sign_l1_action(
            KEY,
            &body["action"],
            None,
            &body["nonce"],
            mainnet,
        )
        .unwrap(),
    )
    .unwrap()
}

#[derive(Default)]
struct BrowserSigner {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for BrowserSigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP relay only")
        };
        let mut body = match &request
            .config
            .data
            .as_ref()
            .unwrap()
            .clone()
            .into_value()
            .unwrap()
        {
            Value::String(text) => serde_json::from_str::<Value>(text).unwrap(),
            body => body.clone(),
        };
        assert!(
            body.get("signature").is_none(),
            "host holds no local secret"
        );
        body["signature"] = sign(&body, !request.config.base_url.contains("testnet"));
        Box::pin(async move {
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                error: None,
                config: Some(SignedConfig {
                    url: None,
                    headers: request.config.headers,
                    params: request.config.params,
                    data: Some(safe_cex_types::retained_json::RetainedJsonInput::Text(
                        body.to_string().encode_utf16().collect(),
                    )),
                }),
            }))
        })
    }
}
impl RemoteSigner for BrowserSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

// Refuse an unexpected origin before network IO, so a broken proxy binding
// fails locally instead of contacting a live venue during the negative control.
struct LocalOnly {
    inner: safe_cex_transport::native::ReqwestAdapter,
    origin: String,
}
impl RestAdapter for LocalOnly {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            if !request
                .base_url
                .as_deref()
                .is_some_and(|url| url.starts_with(&self.origin))
            {
                return Err(RestFailure::pre_transmit(
                    "source origin bypassed the selected proxy".into(),
                    request,
                ));
            }
            self.inner.execute(request).await
        })
    }
}

async fn server(listener: TcpListener, seen: Rc<RefCell<Vec<(String, Value)>>>) {
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        let seen = seen.clone();
        tokio::task::spawn_local(async move {
            let mut data = Vec::new();
            let mut chunk = [0; 4096];
            let end = loop {
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let head = String::from_utf8(data[..end].to_vec()).unwrap();
            let target = head
                .lines()
                .next()
                .unwrap()
                .split(' ')
                .nth(1)
                .unwrap()
                .to_owned();
            let size: usize = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while data.len() < end + size {
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                data.extend_from_slice(&chunk[..n]);
            }
            let body: Value = serde_json::from_slice(&data[end..end + size]).unwrap();
            let response = if target.ends_with("/exchange") {
                assert_eq!(
                    body["signature"],
                    sign(&body, !target.contains("testnet")),
                    "local server verifies the exact signed action/nonce"
                );
                json!({"status":"ok","response":{"type":"scheduleCancel","data":{"status":"success"}}})
            } else {
                json!([])
            };
            seen.borrow_mut().push((target, body));
            let body = response.to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn hyperliquid_registry_uses_real_http_and_hosted_signing_without_a_secret() {
    support::run_local(async {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let seen = Rc::new(RefCell::new(Vec::new()));
            let task = tokio::task::spawn_local(server(listener, seen.clone()));
            let proxy = origin.clone();
            let adapters = Rc::new(move || {
                Ok(Arc::new(LocalOnly {
                    inner: safe_cex_transport::native::reqwest_adapter().unwrap(),
                    origin: origin.clone(),
                }) as Arc<dyn RestAdapter>)
            });
            let limiter = hyperliquid_rate_limiter();
            let signer = Rc::new(BrowserSigner::default());
            let host = native_venue_host_signed(
                safe_cex_exchange::native::native_runtime(),
                adapters.clone(),
                signer.clone(),
                Some("hl-account".into()),
                limiter.clone(),
            );
            let other = native_venue_host_with(
                safe_cex_exchange::native::native_runtime(),
                adapters,
                limiter.clone(),
            );
            // The configured proxy must survive ApiClient's explicit source baseURL.
            for testnet in [false, true] {
                let options: safe_cex_types::ExchangeOptions = serde_json::from_value(json!({
                    "key":ADDRESS,"secret":"","applicationId":ADDRESS,"testnet":testnet,
                    "overrideProxyUrl": proxy
                }))
                .unwrap();
                let deps = host
                    .hyperliquid(ExchangeName::Hyperliquid, &options)
                    .unwrap();
                assert!(Rc::ptr_eq(&deps.limiter, &limiter));
                let spot_deps = host
                    .hyperliquid(ExchangeName::Hyperliquidspot, &options)
                    .unwrap();
                assert!(
                    Rc::ptr_eq(&spot_deps.limiter, &limiter),
                    "Spot and perpetual accounts share one weight budget"
                );
                spot_deps
                    .http
                    .execute(RestRequest {
                        method: "POST".into(),
                        url: "/exchange".into(),
                        base_url: Some(
                            if testnet {
                                "https://api.hyperliquid-testnet.xyz"
                            } else {
                                "https://api.hyperliquid.xyz"
                            }
                            .into(),
                        ),
                        body: Some(safe_cex_transport::RestBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"action":{"type":"scheduleCancel"},"nonce":1700000000000_u64})).unwrap())),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                assert!(
                    Rc::ptr_eq(
                        &other
                            .hyperliquid(ExchangeName::Hyperliquid, &options)
                            .unwrap()
                            .limiter,
                        &limiter
                    ),
                    "distinct account hosts share one weight budget"
                );
                let exchange = rust_constructor(&ExchangeName::Hyperliquid, host.clone())
                    .expect("registry has the bound constructor")
                    .construct(
                        ExchangeName::Hyperliquid,
                        options,
                        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
                    )
                    .unwrap();
                assert_eq!(exchange.name(), &ExchangeName::Hyperliquid);
                assert!(exchange.can_sign());
                assert!(exchange.base().options().secret.is_empty());
                exchange.schedule_cancel(None).await.unwrap();
                assert!(exchange.fetch_orders(None).await.unwrap().is_empty());
                let before = signer.calls.borrow().len();
                deps.http
                    .execute(RestRequest {
                        method: "POST".into(),
                        url: "/info".into(),
                        base_url: Some("https://api.hyperliquid.xyz".into()),
                        skip_remote_signing: true,
                        params: Some(json!({"coin":["BTC","ETH"]})),
                        body: Some(safe_cex_transport::RestBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"type":"allMids"})).unwrap())),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                assert_eq!(
                    signer.calls.borrow().len(),
                    before,
                    "info/query reads skip the browser relay"
                );
                exchange.dispose().unwrap();
            }
            let calls = signer.calls.borrow();
            assert_eq!(calls.len(), 4, "only trading calls go to the remote signer");
            assert_eq!(
                calls
                    .iter()
                    .filter(|call| call["exchangeName"] == "hyperliquidspot")
                    .count(),
                2,
                "Spot host adapter preserves the selected registry name"
            );
            for call in calls.iter() {
                assert!(
                    call["exchangeName"] == "hyperliquid"
                        || call["exchangeName"] == "hyperliquidspot"
                );
                assert_eq!(call["accountName"], "hl-account");
                assert_eq!(call["config"]["url"], "/exchange");
            }
            let seen = seen.borrow();
            for source in [
                "https://api.hyperliquid.xyz",
                "https://api.hyperliquid-testnet.xyz",
            ] {
                assert!(
                    seen.iter()
                        .any(|(path, _)| path == &format!("/{source}/exchange"))
                );
                assert!(
                    seen.iter()
                        .any(|(path, _)| path == &format!("/{source}/info?coin=BTC&coin=ETH")),
                    "qs repeat serialization over the selected proxy"
                );
            }
            task.abort();
        })
        .await
        .expect("local Hyperliquid HTTP completes within 15 seconds");
    });
}
